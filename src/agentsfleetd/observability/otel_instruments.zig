//! Comptime-generated metric instruments for every fixed-label runtime family.
//!
//! The family registry (`otel_metrics_families.zig`) declares each family's
//! label dimensions; this file derives everything else from that one table —
//! a flat atomic cell array sized as the sum of every family's dimension
//! product, a typed writer whose label struct is generated from the declared
//! enums (a wrong or missing dimension is a compile error, replacing the
//! retired comment-enforced value-to-label order pairing), snapshot reads for
//! tests and regenerated module snapshots, and the flush-time collect loop
//! that emits one sample per cell into the aggregator.
//!
//! Families the registry marks `cost` (evented ring), `streamed` (per-runner
//! slot table), or `live_read` (flush-time hooks) have no cells here; the
//! collect loop runs the caller's hooks after the generated cells so hooked
//! families join the same flush window.

const std = @import("std");
const families = @import("otel_metrics_families.zig");
const dims_mod = @import("otel_metrics_dims.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");

const MetricId = families.MetricId;

fn isCellFamily(comptime id: MetricId) bool {
    const meta = families.metaFor(id);
    return !meta.cost and !meta.streamed and !meta.live_read;
}

/// Total generated cells: every cell family's dimension product, summed.
const TOTAL_CELLS: usize = blk: {
    var total: usize = 0;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        if (isCellFamily(id)) total += dims_mod.fixedDimProduct(dims_mod.dimsFor(id));
    }
    break :blk total;
};

/// First cell of each family in the flat table. Entries for non-cell ids are
/// never read: every reader guards on `isCellFamily` at comptime.
const CELL_OFFSETS = blk: {
    var offsets: [families.METRIC_ID_COUNT]usize = undefined;
    var next: usize = 0;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        offsets[i] = next;
        if (isCellFamily(id)) next += dims_mod.fixedDimProduct(dims_mod.dimsFor(id));
    }
    break :blk offsets;
};

/// Widest fixed-dimension count any cell family declares — the label slots a
/// collect template needs.
const MAX_FIXED_DIMS: usize = blk: {
    var widest: usize = 0;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        if (isCellFamily(id)) widest = @max(widest, dims_mod.dimsFor(id).len);
    }
    break :blk widest;
};

/// ~60 families × ≤2 dims × ≤10 enum fields of density checks.
const EVAL_QUOTA_DIM_AUDIT: u32 = 100_000;

comptime {
    @setEvalBranchQuota(EVAL_QUOTA_DIM_AUDIT);
    // The writer maps @intFromEnum straight to a cell ordinal and the collect
    // templates decompose by field order — sound only while every declared
    // dimension enum is dense (values 0..n-1); a sparse enum would alias a
    // neighbouring family's cells. And the widest family's dimension count
    // must fit the sample's label slots or collect would write past them.
    std.debug.assert(MAX_FIXED_DIMS <= payload.MAX_LABELS);
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        if (!isCellFamily(id)) continue;
        for (dims_mod.dimsFor(id)) |dim| {
            const fields = @typeInfo(dim.fixed.Enum).@"enum".fields;
            for (fields, 0..) |field, ordinal| std.debug.assert(field.value == ordinal);
        }
    }
}

// safe because: each cell is an independent monotonic counter or last-writer-
// wins gauge. The flush thread tolerates reading one cell a few nanoseconds
// after another, and no other memory is published through these atomics.
var g_cells: [TOTAL_CELLS]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** TOTAL_CELLS;

/// The typed label struct of one family: one field per declared fixed
/// dimension, named by the wire key, typed as the declared enum. Callers pass
/// anonymous literals (`.{ .reason = .bad_sig }`); an undeclared field or a
/// value from the wrong enum fails the build.
fn LabelsOf(comptime id: MetricId) type {
    const dims = dims_mod.dimsFor(id);
    comptime var field_names: [dims.len][:0]const u8 = undefined;
    comptime var field_types: [dims.len]type = undefined;
    inline for (dims, 0..) |dim, i| {
        field_names[i] = dim.fixed.key ++ "";
        field_types[i] = dim.fixed.Enum;
    }
    const names = field_names;
    const types = field_types;
    return @Struct(.auto, null, &names, &types, &@splat(.{}));
}

/// Flat index of one (family, labelset) cell: row-major over the declared
/// dimensions in order, matching the emission order `collect` walks.
fn cellIndex(comptime id: MetricId, labels: LabelsOf(id)) usize {
    comptime std.debug.assert(isCellFamily(id));
    var index: usize = 0;
    inline for (comptime dims_mod.dimsFor(id)) |dim| {
        const fixed = dim.fixed;
        const value = @field(labels, fixed.key);
        index = index * @typeInfo(fixed.Enum).@"enum".fields.len + @intFromEnum(value);
    }
    return CELL_OFFSETS[@intFromEnum(id)] + index;
}

/// Add an exact count to one monotonic sum cell.
pub fn add(comptime id: MetricId, labels: LabelsOf(id), delta: u64) void {
    comptime std.debug.assert(families.metaFor(id).kind == .sum);
    _ = g_cells[cellIndex(id, labels)].fetchAdd(delta, .monotonic); // safe because: see module note above
}

/// Count one event on a monotonic sum cell.
pub fn inc(comptime id: MetricId, labels: LabelsOf(id)) void {
    add(id, labels, 1);
}

/// Replace one gauge cell's level.
pub fn set(comptime id: MetricId, labels: LabelsOf(id), value: u64) void {
    comptime std.debug.assert(families.metaFor(id).kind == .gauge);
    g_cells[cellIndex(id, labels)].store(value, .release); // safe because: lone gauge publish, last-writer-wins; readers load with .acquire
}

/// Read one cell — the surface regenerated module snapshots and tests read
/// through, so an asserted value is exactly what collect would emit.
pub fn snapshotCell(comptime id: MetricId, labels: LabelsOf(id)) u64 {
    return g_cells[cellIndex(id, labels)].load(.acquire); // safe because: pairs with the writers' release/monotonic stores; staleness is acceptable
}

/// One interned label template per cell, in the same row-major order as
/// `cellIndex` — precomputed so the flush loop only loads and copies.
const CellLabels = struct {
    labels: [MAX_FIXED_DIMS]payload.Label,
    len: u8,
};

/// ~180 cells × ≤2 dims × ~120-entry value-table scans × ~24-byte compares
/// ≈ 1M branches; next power of ten above.
const EVAL_QUOTA_CELL_LABELS: u32 = 10_000_000;

const CELL_LABELS: [TOTAL_CELLS]CellLabels = blk: {
    @setEvalBranchQuota(EVAL_QUOTA_CELL_LABELS);
    var table: [TOTAL_CELLS]CellLabels = undefined;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        if (!isCellFamily(id)) continue;
        const dims = dims_mod.dimsFor(id);
        const product = dims_mod.fixedDimProduct(dims);
        for (0..product) |cell| {
            // SAFETY: the dimension loop below writes slots [0, len) before the
            // entry lands in the table; slots past len are never read.
            var entry = CellLabels{ .labels = undefined, .len = dims.len };
            // Decompose the flat cell ordinal back into per-dimension value
            // ordinals (row-major, first dimension slowest).
            var remainder = cell;
            var stride = product;
            for (dims, 0..) |dim, d| {
                const value_count = @typeInfo(dim.fixed.Enum).@"enum".fields.len;
                stride /= value_count;
                const value_ordinal = remainder / stride;
                remainder %= stride;
                entry.labels[d] = .{
                    .key_idx = dims_mod.keyIndexOf(dim.fixed.key),
                    .val_idx = dims_mod.internedValueIndex(dims_mod.dimValueStrings(dim.fixed.Enum)[value_ordinal]),
                };
            }
            table[CELL_OFFSETS[i] + cell] = entry;
        }
    }
    break :blk table;
};

/// A flush-time source that reads live state (pool statistics, resident-set
/// probe, flush-thread liveness) instead of a generated cell.
pub const CollectHook = *const fn (*aggregate.Aggregator) void;

/// Emit one sample per generated cell — zero values included, so a dashboard
/// series stays live between increments — then run the live-read hooks, after
/// the cells, so hooked families join the same flush window.
pub fn collect(agg: *aggregate.Aggregator, hooks: []const CollectHook) void {
    inline for (0..families.METRIC_ID_COUNT) |i| {
        const id: MetricId = comptime @enumFromInt(i);
        if (comptime isCellFamily(id)) {
            const offset = CELL_OFFSETS[i];
            const product = comptime dims_mod.fixedDimProduct(dims_mod.dimsFor(id));
            for (0..product) |cell| {
                const template = CELL_LABELS[offset + cell];
                var sample = payload.newSample(id, payload.satCast(g_cells[offset + cell].load(.acquire))); // safe because: see module note above
                for (template.labels[0..template.len], 0..) |label, n| sample.labels[n] = label;
                sample.label_count = template.len;
                agg.add(sample);
            }
        }
    }
    for (hooks) |hook| hook(agg);
}

/// Zero the given families' cells between deterministic tests. Scoped per
/// family so one module's reset cannot erase another suite's expectations —
/// reset only families your own suite asserts exactly; assertions on families
/// other suites may touch stay delta-based (before/after snapshots).
pub fn resetCellsForTest(comptime ids: []const MetricId) void {
    inline for (ids) |id| {
        // A non-cell id would alias the next family's offset and zero its
        // first cell; refuse at comptime like every other reader.
        comptime std.debug.assert(isCellFamily(id));
        const offset = CELL_OFFSETS[@intFromEnum(id)];
        const product = comptime dims_mod.fixedDimProduct(dims_mod.dimsFor(id));
        for (0..product) |cell| g_cells[offset + cell].store(0, .release); // safe because: test-only reset between serial tests
    }
}

test {
    _ = @import("otel_instruments_test.zig");
}
