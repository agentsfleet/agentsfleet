const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const otel_logs = @import("otel_logs.zig");

const Ring = otel_metrics.TestRing;
const BUFFER_CAPACITY = otel_metrics.TEST_BUFFER_CAPACITY;

// Pinned label values reused across tests — literal is the contract.
const POSTURE = "standard";
const MODEL = "claude-opus-4-8";

fn sampleWithLabels(id: payload.MetricId, value: i64, ts: u64) payload.Sample {
    var s = payload.newSample(id, value, ts);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, POSTURE);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, MODEL);
    return s;
}

test "ring push/pop round-trip preserves a sample" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const s = sampleWithLabels(.credit_drain, 123456, 1000);
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

    const s = sampleWithLabels(.tokens, 1, 0);
    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(s)); // returns immediately, never blocks
    }
    // Full: the next push drops the sample and bumps the counter, still returns.
    try std.testing.expect(!ring.push(s));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "addLabel respects max count and rejects overflow" {
    var s = payload.newSample(.tokens, 1, 0);
    var i: usize = 0;
    while (i < payload.MAX_LABELS) : (i += 1) {
        try std.testing.expect(payload.addLabel(&s, "k", "v"));
    }
    try std.testing.expect(!payload.addLabel(&s, "overflow", "x"));
    try std.testing.expectEqual(@as(u8, payload.MAX_LABELS), s.label_count);
}

test "addLabel rejects an oversized key or value without partial write" {
    var s = payload.newSample(.tokens, 1, 0);
    const huge_val = "v" ** (payload.MAX_LABEL_VAL + 1);
    try std.testing.expect(!payload.addLabel(&s, payload.LABEL_MODEL, huge_val));
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
    otel_metrics.recordCreditDrain(100, POSTURE, MODEL, "ws-a");
    otel_metrics.recordTokens(7, payload.DIRECTION_INPUT, POSTURE, MODEL);
    otel_metrics.observeRunDuration(42, POSTURE, MODEL);
    // No sample enqueued because every record path early-returns on !isInstalled().
    try std.testing.expectEqual(before, otel_metrics.testPendingCount());
}

test "test_otlp_payload_shape: batch serializes to the pinned OTLP-JSON fixture" {
    const alloc = std.testing.allocator;
    // pin test: literal is the contract — these values + timestamps are what
    // tests/fixtures/telemetry/otlp_metrics.json encodes.
    const samples = [_]payload.Sample{
        // pin test: literal is the contract
        sampleWithLabels(.credit_drain, 123456, 1000),
        blk: {
            // pin test: literal is the contract
            var s = payload.newSample(.tokens, 42, 2000);
            _ = payload.addLabel(&s, payload.LABEL_DIRECTION, payload.DIRECTION_INPUT);
            _ = payload.addLabel(&s, payload.LABEL_POSTURE, POSTURE);
            _ = payload.addLabel(&s, payload.LABEL_MODEL, MODEL);
            break :blk s;
        },
        // pin test: literal is the contract
        sampleWithLabels(.run_duration, 37, 3000),
    };

    const body = try payload.serializeBatch(alloc, "agentsfleetd", &samples);
    defer alloc.free(body);

    const fixture = @embedFile("otlp_metrics.json");
    const want = std.mem.trimEnd(u8, fixture, "\n");
    try std.testing.expectEqualStrings(want, body);

    // Belt-and-braces: the serialized body must be valid JSON.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

test "test_uninstall_joins_cleanly: install then uninstall completes with no hang" {
    const cfg: otel_logs.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:0",
        .instance_id = "test-instance",
        .api_key = "test-key",
    };
    otel_metrics.install(cfg);
    try std.testing.expect(otel_metrics.isInstalled());
    // Empty ring → the flush thread never POSTs; uninstall wakes the tick sleep
    // and joins within one SLEEP_TICK_MS, leaving the exporter disabled.
    otel_metrics.uninstall();
    try std.testing.expect(!otel_metrics.isInstalled());
}

test "test_workspace_label_cardinality_capped: distinct workspaces bounded by the cap" {
    cardinality.reset();
    defer cardinality.reset();

    var buf: [32]u8 = undefined;
    var i: usize = 0;
    while (i < cardinality.WORKSPACE_CARDINALITY_CAP) : (i += 1) {
        const ws = try std.fmt.bufPrint(&buf, "ws-{d}", .{i});
        try std.testing.expect(cardinality.allowWorkspace(ws)); // under cap → retained
    }
    try std.testing.expectEqual(cardinality.WORKSPACE_CARDINALITY_CAP, cardinality.trackedCount());

    // A brand-new workspace beyond the cap is dropped...
    try std.testing.expect(!cardinality.allowWorkspace("ws-overflow"));
    // ...but an already-tracked workspace is still retained.
    const seen = try std.fmt.bufPrint(&buf, "ws-{d}", .{0});
    try std.testing.expect(cardinality.allowWorkspace(seen));
    // Count never grows past the cap.
    try std.testing.expectEqual(cardinality.WORKSPACE_CARDINALITY_CAP, cardinality.trackedCount());
}
