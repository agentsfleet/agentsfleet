//! Tests for the registry-generated instrument layer: registry dimension
//! invariants, typed-writer cell binding, concurrent increment integrity, the
//! zero-cell emission dashboards rely on, and hook ordering. Registered via
//! the `test` block in otel_instruments.zig.

const std = @import("std");
const families = @import("otel_metrics_families.zig");
const dims = @import("otel_metrics_dims.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const instruments = @import("otel_instruments.zig");
const mc = @import("metrics_counters.zig");
const mt = @import("metrics_trace.zig");
const ls = @import("library_stages.zig");
const mrp = @import("metrics_redis_pool.zig");
const runtime = @import("otel_metrics_runtime.zig");

const REDIS_POOL_FAMILY_PREFIX = "agentsfleet_redis_pool_";

fn collectedSeries(agg: *aggregate.Aggregator, buf: []payload.Series) []payload.Series {
    return agg.toSeries(buf);
}

test "test_registry_dimension_product_matches_max_series" {
    // Literal expected products, independent of the registry's own derivation
    // (which computes max_series from the same dims table) — so an accidental
    // dimension or enum-membership change goes red here instead of silently
    // rederiving. pin test: literal is the contract, each line.
    try std.testing.expectEqual(@as(usize, 5), families.metaFor(.http_trace_suppressed).max_series);
    try std.testing.expectEqual(@as(usize, 3), families.metaFor(.otlp_queue_depth).max_series);
    try std.testing.expectEqual(@as(usize, 18), families.metaFor(.otlp_entries_discarded).max_series);
    try std.testing.expectEqual(@as(usize, 6), families.metaFor(.otel_attribute_omitted).max_series);
    try std.testing.expectEqual(@as(usize, 6), families.metaFor(.signup_failed).max_series);
    try std.testing.expectEqual(@as(usize, 30), families.metaFor(.library_stage_duration).max_series);
    try std.testing.expectEqual(@as(usize, 30), families.metaFor(.library_stage_observations).max_series);
    try std.testing.expectEqual(@as(usize, 27), families.metaFor(.library_read_outcome).max_series);
    try std.testing.expectEqual(@as(usize, 4), families.metaFor(.library_pool_result).max_series);
    try std.testing.expectEqual(@as(usize, 5), families.metaFor(.library_cache_outcome).max_series);
    try std.testing.expectEqual(@as(usize, 3), families.metaFor(.library_payload_bytes).max_series);
    try std.testing.expectEqual(@as(usize, 3), families.metaFor(.library_results).max_series);
    // Unlabelled fixed families stay single-series.
    try std.testing.expectEqual(@as(usize, 1), families.metaFor(.api_backpressure_rejections).max_series);
    try std.testing.expectEqual(@as(usize, 1), families.metaFor(.memory_entries_captured).max_series);
}

test "tag-name-derived wire label values stay pinned as literals" {
    // These dimensions derive wire values from enum tag names, so a member
    // rename (or an added label() decl) would silently rename dashboard-facing
    // series. The literals are the freeze — they must never track the enums.
    const SIGNAL_PIN = [_][]const u8{ "logs", "traces", "metrics" }; // pin test: literal is the contract
    const DISCARD_PIN = [_][]const u8{ "ring_full", "aggregate_cap", "serialize_failed", "partial_rejected", "export_rejected", "export_uncertain" }; // pin test: literal is the contract
    const SIGNUP_PIN = [_][]const u8{ "bad_sig", "stale_ts", "missing_email", "db_error", "pool_unavailable", "metadata_writeback" }; // pin test: literal is the contract
    const OMISSION_PIN = [_][]const u8{ "unmapped_provider", "budget_exhausted", "value_too_long" }; // pin test: literal is the contract
    inline for (
        .{ @import("metrics_otel.zig").Signal, @import("metrics_otel.zig").DiscardReason, mc.SignupFailReason, @import("metrics_otel.zig").OmissionReason },
        .{ SIGNAL_PIN, DISCARD_PIN, SIGNUP_PIN, OMISSION_PIN },
    ) |E, pins| {
        const wire = comptime dims.dimValueStrings(E);
        inline for (wire, pins) |wire_value, pinned| {
            try std.testing.expectEqualStrings(pinned, wire_value);
        }
    }
}

test "library label projections agree with the wire derivation" {
    // library_stages' *_LABELS arrays are tag-name projections for tests; the
    // wire derives values via dims.dimValueStrings, which honours a label()
    // decl. The library enums declare none today — this pins the agreement so
    // adding label() to one of them cannot silently fork test from wire.
    inline for (
        .{ ls.Surface, ls.Stage, ls.Outcome, ls.Cache, ls.PoolResult },
        .{ ls.SURFACE_LABELS, ls.STAGE_LABELS, ls.OUTCOME_LABELS, ls.CACHE_LABELS, ls.POOL_RESULT_LABELS },
    ) |E, labels| {
        const wire = comptime dims.dimValueStrings(E);
        inline for (wire, labels) |wire_value, projected| {
            try std.testing.expectEqualStrings(wire_value, projected);
        }
    }
}

test "test_registry_refuses_second_dynamic_dimension" {
    // Negative fixture: a family shape carrying two caller-supplied
    // dimensions must be rejected — the sample layout holds exactly one
    // inline dynamic value. The registry's comptime block asserts validDims
    // for every real family, so this proves the guard has teeth.
    const two_dynamics = [_]dims.LabelDim{
        .{ .dynamic = "fixture_model" },
        .{ .dynamic = "fixture_runner" },
    };
    try std.testing.expect(!comptime dims.validDims(&two_dynamics));
    // The boundary case — exactly one dynamic dimension — stays allowed.
    const one_dynamic = [_]dims.LabelDim{.{ .dynamic = "fixture_model" }};
    try std.testing.expect(comptime dims.validDims(&one_dynamic));
}

test "test_instrument_cell_binding_roundtrip" {
    instruments.resetCellsForTest(&.{.signup_failed});
    defer instruments.resetCellsForTest(&.{.signup_failed});

    instruments.add(.signup_failed, .{ .reason = .db_error }, 7);
    try std.testing.expectEqual(@as(u64, 7), instruments.snapshotCell(.signup_failed, .{ .reason = .db_error }));
    // The typed write moved exactly one cell; a sibling reason stayed zero.
    try std.testing.expectEqual(@as(u64, 0), instruments.snapshotCell(.signup_failed, .{ .reason = .bad_sig }));

    // Collect emits the same cell under exactly that family and label pair.
    var agg = aggregate.Aggregator.init();
    instruments.collect(&agg, &.{});
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    var found = false;
    for (collectedSeries(&agg, &buf)) |series| {
        if (series.id != .signup_failed) continue;
        try std.testing.expectEqual(@as(usize, 1), series.labels.len);
        try std.testing.expectEqualStrings(dims.LABEL_REASON, payload.labelKey(series.labels[0]));
        const value = payload.labelValue(series.labels[0], series.dynamic);
        if (std.mem.eql(u8, value, @tagName(mc.SignupFailReason.db_error))) {
            try std.testing.expectEqual(@as(i64, 7), series.sum_value);
            found = true;
        } else {
            try std.testing.expectEqual(@as(i64, 0), series.sum_value);
        }
    }
    try std.testing.expect(found);
}

test "test_instrument_hammer_no_lost_increments" {
    instruments.resetCellsForTest(&.{.sse_dropped_frames});
    defer instruments.resetCellsForTest(&.{.sse_dropped_frames});

    const Hammer = struct {
        fn run(iters: usize) void {
            for (0..iters) |_| instruments.inc(.sse_dropped_frames, .{});
        }
    };
    const thread_count = 8;
    const adds_per_thread = 10_000;
    var threads: [thread_count]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Hammer.run, .{adds_per_thread});
    for (threads) |t| t.join();

    try std.testing.expectEqual(
        @as(u64, thread_count * adds_per_thread),
        instruments.snapshotCell(.sse_dropped_frames, .{}),
    );
}

test "test_collect_emits_zero_cells" {
    instruments.resetCellsForTest(&.{.http_trace_suppressed});
    defer instruments.resetCellsForTest(&.{.http_trace_suppressed});

    var agg = aggregate.Aggregator.init();
    instruments.collect(&agg, &.{});
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    // Every suppression reason emits a zero-valued series — a dashboard series
    // stays live between increments rather than blinking out.
    var emitted: usize = 0;
    for (collectedSeries(&agg, &buf)) |series| {
        if (series.id != .http_trace_suppressed) continue;
        try std.testing.expectEqual(@as(i64, 0), series.sum_value);
        emitted += 1;
    }
    try std.testing.expectEqual(@typeInfo(mt.SuppressionReason).@"enum".fields.len, emitted);
}

test "collect runs live-read hooks after the generated cells" {
    var observed_count_at_hook: usize = 0;
    const Probe = struct {
        var count_at_hook: usize = 0;
        fn hook(agg: *aggregate.Aggregator) void {
            count_at_hook = agg.count;
        }
    };
    Probe.count_at_hook = 0;
    var agg = aggregate.Aggregator.init();
    instruments.collect(&agg, &.{Probe.hook});
    observed_count_at_hook = Probe.count_at_hook;
    // The hook saw every generated cell already in the window: hooked
    // families join the same flush window, after the cells.
    try std.testing.expect(observed_count_at_hook > 0);
    try std.testing.expectEqual(agg.count, observed_count_at_hook);
}

test "test_pool_hook_absent_until_registered" {
    // Absent half: no registered pool → not one redis_pool_* series in the
    // window (absence, never fake zeros). The registered half — a live pool
    // yielding all eight series — is exercised with a real Pool by
    // queue/redis_pool_test.zig against the same collect path.
    mrp.clearRegisteredPool();
    var agg = aggregate.Aggregator.init();
    runtime.collect(&agg);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    for (collectedSeries(&agg, &buf)) |series| {
        const name = families.metaFor(series.id).name;
        try std.testing.expect(!std.mem.startsWith(u8, name, REDIS_POOL_FAMILY_PREFIX));
    }
}
