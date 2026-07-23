const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const otlp_config = @import("otlp/config.zig");

const Ring = otel_metrics.TestRing;
const BUFFER_CAPACITY = otel_metrics.TEST_BUFFER_CAPACITY;

// Pinned label values reused across tests — literal is the contract.
const POSTURE = "standard";

fn sampleWithLabels(id: payload.MetricId, value: i64) payload.Sample {
    var s = payload.newSample(id, value);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, POSTURE);
    return s;
}

// Takes a const pointer: returning a slice into a by-value parameter would
// dangle once the helper returns (the slice points into freed stack).
fn findLabel(s: *const payload.Sample, key: []const u8) ?[]const u8 {
    var i: u8 = 0;
    while (i < s.label_count) : (i += 1) {
        if (std.mem.eql(u8, s.labels[i].key[0..s.labels[i].key_len], key))
            return s.labels[i].val[0..s.labels[i].val_len];
    }
    return null;
}

const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "test-instance",
    .api_key = "test-key",
};

test "ring push/pop round-trip preserves a sample" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const s = sampleWithLabels(.credit_drain, 123456);
    try std.testing.expect(ring.push(s));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(i64, 123456), popped.?.value);
    try std.testing.expectEqual(payload.MetricId.credit_drain, popped.?.id);
    try std.testing.expectEqual(@as(u8, 2), popped.?.label_count);
    try std.testing.expectEqual(@as(usize, 0), ring.len());
}

test "test_enqueue_drops_on_full_never_blocks" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const s = sampleWithLabels(.tokens, 1);
    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(s)); // returns immediately, never blocks
    }
    // Full: the next push drops the sample and bumps the counter, still returns.
    try std.testing.expect(!ring.push(s));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "addLabel respects max count and rejects overflow" {
    var s = payload.newSample(.tokens, 1);
    var i: usize = 0;
    while (i < payload.MAX_LABELS) : (i += 1) {
        try std.testing.expect(payload.addLabel(&s, "k", "v"));
    }
    try std.testing.expect(!payload.addLabel(&s, "overflow", "x"));
    try std.testing.expectEqual(@as(u8, payload.MAX_LABELS), s.label_count);
}

test "addLabel rejects an oversized key or value without partial write" {
    var s = payload.newSample(.tokens, 1);
    const huge_val = "v" ** (payload.MAX_LABEL_VAL + 1);
    try std.testing.expect(payload.addLabel(&s, payload.LABEL_POSTURE, huge_val) == false);
    try std.testing.expectEqual(@as(u8, 0), s.label_count);
}

test "bucketIndex maps observations to the right bucket" {
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(5)); // <= first bound
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(1));
    try std.testing.expectEqual(@as(usize, 3), payload.bucketIndex(37)); // (25, 50]
    // Past the last bound → the trailing +Inf bucket.
    try std.testing.expectEqual(
        @as(usize, payload.DURATION_BUCKET_BOUNDS_MS.len),
        payload.bucketIndex(999_999),
    );
}

test "test_disabled_when_no_config: record* are no-ops when not installed" {
    try std.testing.expect(!otel_metrics.isInstalled());
    const before = otel_metrics.testPendingCount();
    otel_metrics.recordCreditDrain(100, POSTURE);
    otel_metrics.recordTokens(7, payload.DIRECTION_INPUT, POSTURE);
    otel_metrics.observeRunDuration(42, POSTURE);
    // No sample enqueued because every record path early-returns on !isInstalled().
    try std.testing.expectEqual(before, otel_metrics.testPendingCount());
}

test "test_otlp_payload_shape: aggregated series serialize to the pinned OTLP-JSON fixture" {
    const alloc = std.testing.allocator;

    // Build label sets via Sample construction, then view them as aggregated
    // Series. Window = [1000, 2000] (delta temporality, one window stamp).
    const s_credit = sampleWithLabels(.credit_drain, 0);
    var s_tokens = payload.newSample(.tokens, 0);
    _ = payload.addLabel(&s_tokens, payload.LABEL_DIRECTION, payload.DIRECTION_INPUT);
    _ = payload.addLabel(&s_tokens, payload.LABEL_POSTURE, POSTURE);
    const s_dur = sampleWithLabels(.run_duration, 0);
    // One 37ms observation → bucket index 3 (the (25, 50] bucket).
    var dur_buckets = [_]u64{0} ** payload.N_BUCKETS;
    dur_buckets[3] = 1;

    // pin test: literal is the contract — these values are what
    // tests/fixtures/telemetry/otlp_metrics.json encodes.
    const series = [_]payload.Series{
        .{ .id = .credit_drain, .labels = s_credit.labels[0..s_credit.label_count], .sum_value = 123456, .hist_count = 0, .hist_sum = 0, .bucket_counts = &[_]u64{} },
        .{ .id = .tokens, .labels = s_tokens.labels[0..s_tokens.label_count], .sum_value = 42, .hist_count = 0, .hist_sum = 0, .bucket_counts = &[_]u64{} },
        .{ .id = .run_duration, .labels = s_dur.labels[0..s_dur.label_count], .sum_value = 0, .hist_count = 1, .hist_sum = 37, .bucket_counts = &dur_buckets },
    };

    // pin test: literal is the contract (window start/now)
    const body = try payload.serializeSeries(alloc, "agentsfleetd", &series, 1000, 2000);
    defer alloc.free(body);

    const fixture = @embedFile("otlp_metrics.json");
    const want = std.mem.trimEnd(u8, fixture, "\n");
    try std.testing.expectEqualStrings(want, body);

    // Belt-and-braces: the serialized body must be valid JSON.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

test "test_uninstall_joins_cleanly: install then uninstall completes with no hang" {
    const cfg: otlp_config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:0",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };
    _ = otel_metrics.install(@import("common").globalIo(), cfg);
    try std.testing.expect(otel_metrics.isInstalled());
    // Empty ring → the flush thread never POSTs; uninstall wakes the tick sleep
    // and joins within one SLEEP_TICK_MS, leaving the exporter disabled.
    otel_metrics.uninstall();
    try std.testing.expect(!otel_metrics.isInstalled());
}

test "test_emits_credit_drain_on_debit: a committed debit records a credit-drain sum" {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    otel_metrics.recordCreditDrain(500, POSTURE);

    const s = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.credit_drain, s.id);
    try std.testing.expectEqual(@as(i64, 500), s.value);
    try std.testing.expectEqualStrings(POSTURE, findLabel(&s, payload.LABEL_POSTURE).?);
    try std.testing.expect(otel_metrics.testPop() == null);
}

test "test_emits_token_throughput_on_settle: settle records token sums per non-zero direction" {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // cached = 0 → that direction is skipped; charged + input + output + duration emit.
    // pin test: literal is the contract
    otel_metrics.recordRunSettlement(1000, 10, 0, 5, 100, POSTURE);

    // FIFO emission order: credit_drain, tokens(input), tokens(output), run_duration.
    const drain = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.credit_drain, drain.id);
    // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 1000), drain.value);

    const tok_in = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.tokens, tok_in.id);
    try std.testing.expectEqual(@as(i64, 10), tok_in.value);
    try std.testing.expectEqualStrings(payload.DIRECTION_INPUT, findLabel(&tok_in, payload.LABEL_DIRECTION).?);

    const tok_out = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.tokens, tok_out.id);
    try std.testing.expectEqual(@as(i64, 5), tok_out.value);
    try std.testing.expectEqualStrings(payload.DIRECTION_OUTPUT, findLabel(&tok_out, payload.LABEL_DIRECTION).?);

    const dur = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.run_duration, dur.id);
    try std.testing.expectEqual(@as(i64, 100), dur.value);

    // cached=0 was skipped → nothing left.
    try std.testing.expect(otel_metrics.testPop() == null);
}

test "test_observes_run_latency_histogram: observeRunDuration enqueues a histogram sample" {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    otel_metrics.observeRunDuration(37, POSTURE);

    const s = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.run_duration, s.id);
    try std.testing.expectEqual(@as(i64, 37), s.value);
    try std.testing.expectEqualStrings(POSTURE, findLabel(&s, payload.LABEL_POSTURE).?);
}

test "test_dashboard_metric_names_match_constants: every emitted metric name is referenced by the dashboard" {
    const alloc = std.testing.allocator;
    const dash = @embedFile("agent-observability.json");

    // The dashboard is valid JSON.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, dash, .{});
    defer parsed.deinit();

    // Every emitted metric-name constant appears (in __source_otlp_metrics and/or
    // a panel expr) — a metric rename in code breaks this test until the
    // dashboard is updated. The metric names are the single source of truth.
    inline for (.{
        payload.METRIC_CREDIT_DRAIN,
        payload.METRIC_TOKENS,
        payload.METRIC_RUN_DURATION,
        payload.METRIC_SAMPLES_DROPPED,
    }) |name| {
        try std.testing.expect(std.mem.indexOf(u8, dash, name) != null);
    }
}

test "test_window_resets_after_flush: a flush drains + aggregates the window; the next is empty" {
    const alloc = std.testing.allocator;
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // pin test: literal is the contract
    otel_metrics.recordCreditDrain(100, POSTURE);
    // pin test: literal is the contract
    otel_metrics.recordCreditDrain(50, POSTURE);
    try std.testing.expectEqual(@as(usize, 2), otel_metrics.testPendingCount());

    // First flush drains + coalesces the same labelset → one summed dataPoint.
    const body1 = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.ExpectedBody;
    defer alloc.free(body1);
    try std.testing.expect(std.mem.indexOf(u8, body1, "\"asInt\":\"150\"") != null); // 100 + 50

    // Window reset: the ring is drained, so the next flush is empty (delta).
    try std.testing.expectEqual(@as(usize, 0), otel_metrics.testPendingCount());
    const body2 = try otel_metrics.testCollectOnce(alloc, TEST_CFG);
    try std.testing.expect(body2 == null);
}

test "test_samples_dropped_emitted: ring overflow surfaces the samples_dropped self-metric" {
    const alloc = std.testing.allocator;
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // Push past ring capacity → enqueue-time drops; the flush emits the delta as
    // agentsfleet.telemetry.samples_dropped.
    var i: usize = 0;
    while (i < otel_metrics.TEST_BUFFER_CAPACITY + 8) : (i += 1) {
        otel_metrics.recordCreditDrain(1, POSTURE);
    }
    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, payload.METRIC_SAMPLES_DROPPED) != null);
}
