//! Flush-window behaviour for the metrics exporter — the second slice of
//! otel_metrics_test.zig (ring/sample mechanics and wire pins live there;
//! sample-shape and attribution-budget claims in
//! otel_metrics_attribution_test.zig). Registered from tests.zig.

const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const families = @import("otel_metrics_families.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const mrp = @import("metrics_redis_pool.zig");
const semconv = @import("semconv.zig");
const otlp_config = @import("otlp/config.zig");
const health = @import("metrics_otel.zig");
const common = @import("common");

const POSTURE = "platform";
const PROVIDER = "anthropic";
const MODEL = "claude-opus-4-8";
const MODEL_FIXTURE_FMT = "model-{d}";

const ATTR: otel_metrics.Attribution = .{ .posture = POSTURE, .provider = PROVIDER, .model = MODEL };

const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "test-instance",
    .api_key = "test-key",
    .service_version = "0.0.0-test",
};

// ── Dimension 2.1 ──────────────────────────────────────────────────────────

test "test_metric_descriptors_match_semantic_schema" {
    const alloc = std.testing.allocator;
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    cardinality.reset();
    defer cardinality.reset();

    otel_metrics.recordCreditConsumed(500, .settle, ATTR);
    otel_metrics.observeTokenUsage(100, .input, ATTR);
    otel_metrics.observeCacheReadTokens(20, ATTR);
    otel_metrics.observeInvokeAgentDuration(37, null, ATTR);

    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.ExpectedBody;
    defer alloc.free(body);

    // Names + units, exactly as the registry declares them.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"gen_ai.invoke_agent.duration\",\"unit\":\"s\"") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"agentsfleet.invoke_agent.token.usage\",\"unit\":\"{token}\"") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"agentsfleet.invoke_agent.cache_read.token.usage\",\"unit\":\"{token}\"") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"agentsfleet.billing.credit.consumed\",\"unit\":\"{nanocredit}\"") != null); // pin test: literal is the contract

    // Instrument kinds: credit is a monotonic delta sum, the rest are histograms.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"sum\":{\"aggregationTemporality\":1,\"isMonotonic\":true") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"histogram\":{\"aggregationTemporality\":1") != null); // pin test: literal is the contract

    // Seconds on the wire from a millisecond observation, with no float math:
    // 37ms → 0.037s, and the first pinned agent bound 100ms → 0.100s.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"sum\":0.037") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"explicitBounds\":[0.100,0.200,0.400") != null); // pin test: literal is the contract

    // Standard attribute keys, and none of the superseded private ones.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_EXECUTION_POSTURE) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_REQUEST_MODEL) != null);
    for (semconv.METRIC_FORBIDDEN_ATTRS) |forbidden| {
        const quoted = try std.fmt.allocPrint(alloc, "\"key\":\"{s}\"", .{forbidden});
        defer alloc.free(quoted);
        try std.testing.expect(std.mem.indexOf(u8, body, quoted) == null);
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

// ── Exporter lifecycle + flush behaviour ───────────────────────────────────

test "install then uninstall completes with no hang" {
    _ = otel_metrics.install(common.globalIo(), TEST_CFG);
    try std.testing.expect(otel_metrics.isInstalled());
    // Empty ring → the flush thread never POSTs; uninstall wakes the tick sleep
    // and joins within one SLEEP_TICK_MS, leaving the exporter disabled.
    otel_metrics.uninstall();
    try std.testing.expect(!otel_metrics.isInstalled());
}

test "a flush drains and aggregates the window; the next window's delta is empty" {
    const alloc = std.testing.allocator;
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    cardinality.reset();
    defer cardinality.reset();

    // pin test: literal is the contract
    otel_metrics.recordCreditConsumed(100, .renewal, ATTR);
    // pin test: literal is the contract
    otel_metrics.recordCreditConsumed(50, .renewal, ATTR);
    try std.testing.expectEqual(@as(usize, 2), otel_metrics.testPendingCount());

    // First flush drains + coalesces the same labelset → one summed dataPoint.
    const body1 = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.ExpectedBody;
    defer alloc.free(body1);
    try std.testing.expect(std.mem.indexOf(u8, body1, "\"asInt\":\"150\"") != null); // 100 + 50
    try std.testing.expect(std.mem.indexOf(u8, body1, semconv.METRIC_BILLING_CREDIT_CONSUMED) != null);

    // Window reset: the ring is drained, so the next flush carries no evented
    // delta — the credit family is absent even though the runtime snapshot
    // families still ride the envelope every window.
    try std.testing.expectEqual(@as(usize, 0), otel_metrics.testPendingCount());
    const body2 = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.ExpectedBody;
    defer alloc.free(body2);
    try std.testing.expect(std.mem.indexOf(u8, body2, semconv.METRIC_BILLING_CREDIT_CONSUMED) == null);
}

test "ring overflow surfaces the samples_dropped self-metric" {
    const alloc = std.testing.allocator;
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    cardinality.reset();
    defer cardinality.reset();

    // Push past ring capacity → enqueue-time drops; the flush emits the delta as
    // agentsfleet.telemetry.samples_dropped.
    var i: usize = 0;
    while (i < otel_metrics.TEST_BUFFER_CAPACITY + 8) : (i += 1) {
        otel_metrics.recordCreditConsumed(1, .receive, ATTR);
    }
    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.METRIC_SAMPLES_DROPPED) != null);
    const snapshot = health.snapshot();
    try std.testing.expectEqual(@as(u32, otel_metrics.TEST_BUFFER_CAPACITY - 1), otel_metrics.testAcceptedSinceCycle());
    try std.testing.expectEqual(
        @as(u64, 9),
        snapshot.discarded[@intFromEnum(health.Signal.metrics)][@intFromEnum(health.DiscardReason.ring_full)],
    );
}

/// Fixed-label series the redis-pool families would occupy — subtracted from
/// the expected runtime count when no Pool is registered in this process.
const REDIS_POOL_SERIES: usize = blk: {
    var total: usize = 0;
    for (0..families.METRIC_ID_COUNT) |i| {
        const meta = families.metaFor(@enumFromInt(i));
        if (!meta.cost and !meta.streamed and std.mem.startsWith(u8, meta.name, "agentsfleet_redis_pool_")) total += meta.max_series;
    }
    break :blk total;
};

test "the series cap drops surface separately from ring-full drops" {
    const alloc = std.testing.allocator;
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // MAX_SERIES+1 distinct label sets fill every accumulator slot and drop
    // exactly one evented sample at the cap. The runtime snapshot families
    // join the same window AFTER the drain, so with the slots already full
    // every one of them is also dropped at the cap — their exact count is
    // derived from the registry (minus the families this test process cannot
    // produce: an unregistered Pool, an unreportable resident set).
    var buf: [32]u8 = undefined;
    var i: usize = 0;
    while (i < aggregate.MAX_SERIES + 1) : (i += 1) {
        var s = payload.newSample(.credit_consumed, 1);
        _ = payload.setDynamicLabel(&s, semconv.ATTR_REQUEST_MODEL, try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{i}));
        otel_metrics.testPush(s);
    }
    var expected_runtime: u64 = families.RUNTIME_FIXED_SERIES;
    if (mrp.snapshot() == null) expected_runtime -= REDIS_POOL_SERIES;
    if (common.rss.currentBytes() == null) expected_runtime -= 1;

    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.METRIC_SAMPLES_DROPPED) != null);
    const snapshot = health.snapshot();
    // Every drop surfaced under the aggregate cap — none as ring-full.
    try std.testing.expectEqual(
        1 + expected_runtime,
        snapshot.discarded[@intFromEnum(health.Signal.metrics)][@intFromEnum(health.DiscardReason.aggregate_cap)],
    );
    try std.testing.expectEqual(
        @as(u64, 0),
        snapshot.discarded[@intFromEnum(health.Signal.metrics)][@intFromEnum(health.DiscardReason.ring_full)],
    );
}
