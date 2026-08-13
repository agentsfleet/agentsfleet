//! Label dimensions of the closed metric-family registry, plus the interned
//! key/value tables derived from them. Sibling of otel_metrics_families.zig
//! (which owns wire identity and ceiling arithmetic): this file owns which
//! label keys exist, which closed enums dimension each family, and the
//! comptime index tables the compact sample layout resolves against — so a
//! key or closed value cannot reach the wire without a declared home here.
//!
//! Dependency direction rule: the metrics_* modules contribute enum TYPES and
//! name CONSTANTS here, nothing else — dimsFor/KEYS/VALUES must never read
//! families.METAS or instrument state. The mutual imports around this file
//! stay legal only while dependencies are decl-granular; a comptime consult
//! in the other direction creates a dependency loop with an opaque error.

const std = @import("std");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const mot = @import("metrics_otel.zig");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_runner.zig");
const mt = @import("metrics_trace.zig");
const mrv = @import("metrics_repair_verification.zig");
const ls = @import("library_stages.zig");
const Mode = @import("../state/tenant_provider.zig").Mode;

// ---------------------------------------------------------------------------
// Label keys owned by the registry. Library families keep their keys in
// library_stages.zig (that module is the schema for its own dimensions);
// evented cost families use semconv's attribute keys. The generic
// reason/signal/attribute/runner keys have their one home here; `outcome` is
// imported from library_stages.zig, its declaration site.
// ---------------------------------------------------------------------------

pub const LABEL_REASON = "reason";
pub const LABEL_SIGNAL = "signal";
pub const LABEL_ATTRIBUTE = "attribute";
pub const LABEL_RUNNER = "runner_id";
/// Same wire key as the library outcome dimension; imported so the string
/// keeps exactly one declaration site.
pub const LABEL_OUTCOME = ls.LABEL_OUTCOME;

/// One label dimension of a family. Comptime-only (a `fixed` dimension names
/// the enum type whose members are the label values); runtime readers see the
/// derived `max_series` on `MetricMeta` instead.
pub const LabelDim = union(enum) {
    fixed: struct { key: []const u8, Enum: type },
    /// Wire key of the at-most-one caller-supplied dimension (request model,
    /// runner identifier). Its value rides the sample's single inline buffer.
    dynamic: []const u8,
};

/// At most one dynamic dimension per family: a sample carries exactly one
/// inline dynamic value, so a second dynamic dimension has nowhere to live.
pub const MAX_DYNAMIC_DIMS: usize = 1;

/// True when the dimension list fits the sample layout. Kept as a pure
/// predicate (rather than asserting inline) so the negative case is testable:
/// the registry comptime block asserts it for every declared family, and the
/// instrument test proves a two-dynamic declaration is refused.
pub fn validDims(comptime dims: []const LabelDim) bool {
    var dynamic_count: usize = 0;
    for (dims) |dim| {
        if (dim == .dynamic) dynamic_count += 1;
    }
    return dynamic_count <= MAX_DYNAMIC_DIMS;
}

/// Comptime product of the fixed dimensions' value counts — the exact number
/// of storage cells and worst-case series a fixed-label family occupies.
pub fn fixedDimProduct(comptime dims: []const LabelDim) usize {
    var product: usize = 1;
    for (dims) |dim| switch (dim) {
        .fixed => |f| product *= @typeInfo(f.Enum).@"enum".fields.len,
        .dynamic => {},
    };
    return product;
}

/// Wire label values of one fixed dimension, in enum declaration order. An
/// enum that defines `label()` (wire spelling differs from the tag name, e.g.
/// the omitted-attribute keys) resolves through it; plain enums use tag names.
pub fn dimValueStrings(comptime E: type) [@typeInfo(E).@"enum".fields.len][]const u8 {
    const fields = @typeInfo(E).@"enum".fields;
    var out: [fields.len][]const u8 = undefined;
    for (fields, 0..) |f, i| {
        const member: E = @enumFromInt(f.value);
        out[i] = if (@hasDecl(E, "label")) member.label() else f.name;
    }
    return out;
}

/// The label dimensions of every family, in wire emission order. This is the
/// single source the storage layout, the typed writer, the collect loop, and
/// each fixed-label family's `max_series` derive from — the value-to-label
/// binding is the type system's, not a comment's.
pub fn dimsFor(comptime id: families.MetricId) []const LabelDim {
    return switch (id) {
        .invoke_agent_duration, .token_usage, .cache_read_token_usage, .credit_consumed => &.{.{ .dynamic = semconv.ATTR_REQUEST_MODEL }},
        .http_trace_suppressed => &.{.{ .fixed = .{ .key = LABEL_REASON, .Enum = mt.SuppressionReason } }},
        .otlp_queue_depth => &.{.{ .fixed = .{ .key = LABEL_SIGNAL, .Enum = mot.Signal } }},
        .otlp_entries_discarded => &.{ .{ .fixed = .{ .key = LABEL_SIGNAL, .Enum = mot.Signal } }, .{ .fixed = .{ .key = LABEL_REASON, .Enum = mot.DiscardReason } } },
        .otel_attribute_omitted => &.{ .{ .fixed = .{ .key = LABEL_ATTRIBUTE, .Enum = mot.OmittedAttribute } }, .{ .fixed = .{ .key = LABEL_REASON, .Enum = mot.OmissionReason } } },
        .signup_failed => &.{.{ .fixed = .{ .key = LABEL_REASON, .Enum = mc.SignupFailReason } }},
        .repair_provider_results => &.{.{ .fixed = .{ .key = LABEL_OUTCOME, .Enum = mrv.ProviderResult } }},
        .repair_correlations => &.{.{ .fixed = .{ .key = LABEL_OUTCOME, .Enum = mrv.Correlation } }},
        .repair_synthetic_events => &.{.{ .fixed = .{ .key = LABEL_OUTCOME, .Enum = mrv.EventOutcome } }},
        .repair_verifier_runs => &.{.{ .fixed = .{ .key = LABEL_OUTCOME, .Enum = mrv.VerifierOutcome } }},
        .library_stage_duration, .library_stage_observations => &.{ .{ .fixed = .{ .key = ls.LABEL_SURFACE, .Enum = ls.Surface } }, .{ .fixed = .{ .key = ls.LABEL_STAGE, .Enum = ls.Stage } } },
        .library_read_outcome => &.{ .{ .fixed = .{ .key = ls.LABEL_SURFACE, .Enum = ls.Surface } }, .{ .fixed = .{ .key = ls.LABEL_OUTCOME, .Enum = ls.Outcome } } },
        .library_pool_result => &.{.{ .fixed = .{ .key = ls.LABEL_POOL_RESULT, .Enum = ls.PoolResult } }},
        .library_cache_outcome => &.{.{ .fixed = .{ .key = ls.LABEL_CACHE, .Enum = ls.Cache } }},
        .library_payload_bytes, .library_results => &.{.{ .fixed = .{ .key = ls.LABEL_SURFACE, .Enum = ls.Surface } }},
        .runner_failures, .runner_executions, .runner_last_seen_seconds, .runner_active_leases => &.{.{ .dynamic = LABEL_RUNNER }},
        else => &.{},
    };
}

// ---------------------------------------------------------------------------
// Interned label tables — derived from the declarations above, so a key or
// closed value cannot reach the wire without a declared home.
// ---------------------------------------------------------------------------

// Comptime evaluation budgets for the table builds and lookups below —
// branch-count math lives beside each use.
const EVAL_QUOTA_KEY_TABLE: u32 = 100_000;
const EVAL_QUOTA_VALUE_TABLE: u32 = 1_000_000;
const EVAL_QUOTA_VALUE_LOOKUP: u32 = 100_000;

fn containsString(comptime table: []const []const u8, comptime s: []const u8) bool {
    for (table) |entry| {
        if (std.mem.eql(u8, entry, s)) return true;
    }
    return false;
}

fn dedup(comptime raw: []const []const u8) []const []const u8 {
    var out: []const []const u8 = &.{};
    for (raw) |s| {
        if (!containsString(out, s)) out = out ++ &[_][]const u8{s};
    }
    return out;
}

/// Label keys the evented record API attaches beyond the registry dimensions.
const EVENTED_ATTR_KEYS = [_][]const u8{
    semconv.ATTR_OPERATION_NAME,
    semconv.ATTR_PROVIDER_NAME,
    semconv.ATTR_TOKEN_TYPE,
    semconv.ATTR_ERROR_TYPE,
    semconv.ATTR_EXECUTION_POSTURE,
    semconv.ATTR_CHARGE_TYPE,
};

/// Every label key any family can put on the wire, deduplicated.
pub const KEYS = blk: {
    // ~20 keys × ~20-entry dedup scans × ~30-byte compares ≈ 12k branches;
    // next power of ten above with headroom for registry growth.
    @setEvalBranchQuota(EVAL_QUOTA_KEY_TABLE);
    var raw: []const []const u8 = &EVENTED_ATTR_KEYS;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: families.MetricId = @enumFromInt(i);
        for (dimsFor(id)) |dim| switch (dim) {
            .fixed => |f| raw = raw ++ &[_][]const u8{f.key},
            .dynamic => |key| raw = raw ++ &[_][]const u8{key},
        };
    }
    break :blk dedup(raw);
};

/// Values that belong to no closed enum: the operation name, the well-known
/// provider spellings, and the per-runner reason/outcome labels the streamed
/// path interns at comptime. These occupy the table's leading block.
const OPERATION_VALUES = [_][]const u8{semconv.OPERATION_INVOKE_AGENT};
const LITERAL_VALUES = OPERATION_VALUES ++
    semconv.WELL_KNOWN_PROVIDERS ++
    mr.REASON_LABELS ++ mr.OUTCOME_LABELS;

/// Closed enums the evented record API attaches beyond the registry's fixed
/// dimensions. The fixed-dimension enums are discovered from `dimsFor`, so only
/// the evented ones are named here.
const EVENTED_ENUMS = [_]type{ semconv.TokenType, semconv.ChargeClass, Mode, semconv.ErrorType };

fn containsType(comptime table: []const type, comptime T: type) bool {
    for (table) |entry| {
        if (entry == T) return true;
    }
    return false;
}

/// Every closed enum whose members can reach the wire, in block order: the
/// evented enums first, then each family's fixed dimensions in registry order.
/// An enum dimensioning two families occupies exactly one block.
const CLOSED_ENUMS: []const type = blk: {
    @setEvalBranchQuota(EVAL_QUOTA_VALUE_TABLE);
    var out: []const type = &EVENTED_ENUMS;
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: families.MetricId = @enumFromInt(i);
        for (dimsFor(id)) |dim| switch (dim) {
            .fixed => |f| if (!containsType(out, f.Enum)) {
                out = out ++ &[_]type{f.Enum};
            },
            .dynamic => {},
        };
    }
    break :blk out;
};

/// Base index of each closed enum's block, parallel to `CLOSED_ENUMS`.
const ENUM_BASE: [CLOSED_ENUMS.len]u16 = blk: {
    var bases: [CLOSED_ENUMS.len]u16 = undefined;
    var next: usize = LITERAL_VALUES.len;
    for (CLOSED_ENUMS, &bases) |E, *base| {
        base.* = @intCast(next);
        next += @typeInfo(E).@"enum".fields.len;
    }
    break :blk bases;
};

/// Index of the first well-known provider, derived from its position in the
/// literal block so that adding a literal cannot silently shift the providers.
/// Derived from the block layout rather than by searching for a provider
/// spelling: a literal that happened to equal a provider name would otherwise
/// bind this to the wrong index.
const PROVIDER_BASE: u16 = OPERATION_VALUES.len;

/// Every label value any family can put on the wire: the literal block, then
/// each closed enum's members in declaration order. Deliberately not
/// deduplicated — a shared spelling across two enums keeps two indices so that
/// every member stays at its own block's base plus its ordinal.
pub const VALUES = blk: {
    @setEvalBranchQuota(EVAL_QUOTA_VALUE_TABLE);
    var out: []const []const u8 = &LITERAL_VALUES;
    for (CLOSED_ENUMS) |E| {
        for (dimValueStrings(E)) |v| out = out ++ &[_][]const u8{v};
    }
    break :blk out;
};

comptime {
    @setEvalBranchQuota(EVAL_QUOTA_VALUE_TABLE);
    std.debug.assert(KEYS.len <= std.math.maxInt(u8));
    std.debug.assert(VALUES.len <= std.math.maxInt(u16));
    // Ordinal indexing is sound only while every closed enum numbers its
    // members densely from zero: an explicitly-valued or sparse enum would let
    // `@intFromEnum` index past its own block and into its neighbour's.
    for (CLOSED_ENUMS) |E| {
        for (@typeInfo(E).@"enum".fields, 0..) |f, i| std.debug.assert(f.value == i);
    }
    // Blocks are contiguous and disjoint, and together with the literal block
    // they exactly cover the table.
    var expected: usize = LITERAL_VALUES.len;
    for (CLOSED_ENUMS, ENUM_BASE) |E, base| {
        std.debug.assert(base == expected);
        expected += @typeInfo(E).@"enum".fields.len;
    }
    std.debug.assert(expected == VALUES.len);
}

/// Comptime index of a registered label key; unknown keys fail the build, so
/// a writer physically cannot attach an undeclared key.
pub fn keyIndexOf(comptime key: []const u8) u8 {
    for (KEYS, 0..) |k, i| {
        if (std.mem.eql(u8, k, key)) return @intCast(i);
    }
    @compileError("label key has no registry home: " ++ key);
}

/// Comptime index of a registered closed label value.
pub fn internedValueIndex(comptime val: []const u8) u16 {
    // One ~120-entry scan × ~24-byte compares per comptime lookup site.
    @setEvalBranchQuota(EVAL_QUOTA_VALUE_LOOKUP);
    for (VALUES, 0..) |v, i| {
        if (std.mem.eql(u8, v, val)) return @intCast(i);
    }
    @compileError("label value is not a declared closed value: " ++ val);
}

/// Comptime base index of a closed enum's block. An enum with no block fails
/// the build, so a writer physically cannot attach an unregistered value — the
/// runtime miss this replaces returned false and was discarded by every caller.
pub fn baseOf(comptime E: type) u16 {
    // `inline` because the element type is `type`: an ordinary loop index is
    // runtime-known, and a type cannot be selected at runtime.
    inline for (CLOSED_ENUMS, ENUM_BASE) |T, base| {
        if (T == E) return base;
    }
    @compileError("closed enum has no registry home: " ++ @typeName(E));
}

/// Interned index of one closed enum member: one add, no search.
pub fn valueIndexOf(comptime E: type, value: E) u16 {
    return baseOf(E) + @intFromEnum(value);
}

/// Interned index of a well-known provider, resolved from the ordinal
/// `semconv.providerOrdinal` already computed while normalizing.
pub fn providerValueIndex(ordinal: u16) u16 {
    return PROVIDER_BASE + ordinal;
}
