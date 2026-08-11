//! Ring/sample mechanics and wire-shape pins for the metrics exporter. The
//! flush-window and attribution-budget behaviour lives in
//! otel_metrics_flush_test.zig (registered from tests.zig).

const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const families = @import("otel_metrics_families.zig");
const semconv = @import("semconv.zig");
const otlp_config = @import("otlp/config.zig");

const Ring = otel_metrics.TestRing;
const BUFFER_CAPACITY = otel_metrics.TEST_BUFFER_CAPACITY;

// Fixture identity. The posture is a real resolver label, not an invented one,
// so the attribute values these tests pin are the values production emits.
const POSTURE = "platform";
const PROVIDER = "anthropic";
const MODEL = "claude-opus-4-8";

const ATTR: otel_metrics.Attribution = .{ .posture = POSTURE, .provider = PROVIDER, .model = MODEL };

const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "test-instance",
    .api_key = "test-key",
    .service_version = "0.0.0-test",
};

fn sampleWithLabels(id: payload.MetricId, value: i64) payload.Sample {
    var s = payload.newSample(id, value);
    _ = payload.addLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
    return s;
}

// ── Ring + sample mechanics ────────────────────────────────────────────────

test "ring push/pop round-trip preserves a sample" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const s = sampleWithLabels(.credit_consumed, 123456);
    try std.testing.expect(ring.push(s));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(i64, 123456), popped.?.value);
    try std.testing.expectEqual(payload.MetricId.credit_consumed, popped.?.id);
    try std.testing.expectEqual(@as(u8, 2), popped.?.label_count);
    try std.testing.expectEqual(@as(usize, 0), ring.len());
}

test "enqueue drops on full and never blocks" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const s = sampleWithLabels(.token_usage, 1);
    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(s)); // returns immediately, never blocks
    }
    // Full: the next push drops the sample and bumps the counter, still returns.
    try std.testing.expect(!ring.push(s));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "addLabel respects max count and rejects overflow" {
    var s = payload.newSample(.token_usage, 1);
    var i: usize = 0;
    while (i < payload.MAX_LABELS) : (i += 1) {
        try std.testing.expect(payload.addLabel(&s, "k", "v"));
    }
    try std.testing.expect(!payload.addLabel(&s, "overflow", "x"));
    try std.testing.expectEqual(@as(u8, payload.MAX_LABELS), s.label_count);
}

test "addLabel rejects an oversized key or value without partial write" {
    var s = payload.newSample(.token_usage, 1);
    const huge_val = "v" ** (payload.MAX_LABEL_VAL + 1);
    try std.testing.expect(!payload.valueFits(huge_val));
    try std.testing.expect(!payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, huge_val));
    try std.testing.expectEqual(@as(u8, 0), s.label_count);

    const huge_key = "k" ** (payload.MAX_LABEL_KEY + 1);
    try std.testing.expect(!payload.addLabel(&s, huge_key, "v"));
    try std.testing.expectEqual(@as(u8, 0), s.label_count);
}

test "every live attribute key fits the payload key bound" {
    // The bound is what makes the sample fixed-size; a registry key that did not
    // fit would be silently dropped from every point that carries it.
    inline for (.{
        semconv.ATTR_OPERATION_NAME,
        semconv.ATTR_PROVIDER_NAME,
        semconv.ATTR_REQUEST_MODEL,
        semconv.ATTR_TOKEN_TYPE,
        semconv.ATTR_ERROR_TYPE,
        semconv.ATTR_EXECUTION_POSTURE,
        semconv.ATTR_CHARGE_TYPE,
    }) |key| {
        try std.testing.expect(key.len <= payload.MAX_LABEL_KEY);
    }
}

test "bucketIndex maps observations to the right bucket" {
    const duration = families.metaFor(.invoke_agent_duration).bounds;
    // Milliseconds in, because that is what the runner reports.
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(5, duration)); // <= first bound (100ms)
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(100, duration)); // inclusive upper edge
    try std.testing.expectEqual(@as(usize, 2), payload.bucketIndex(370, duration)); // (200, 400]
    // Past the last bound → the trailing +Inf bucket.
    try std.testing.expectEqual(@as(usize, duration.len), payload.bucketIndex(999_999, duration));

    const tokens = families.metaFor(.token_usage).bounds;
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(1, tokens));
    try std.testing.expectEqual(@as(usize, tokens.len), payload.bucketIndex(1 << 40, tokens));
}

test "record paths are no-ops when the exporter is not installed" {
    try std.testing.expect(!otel_metrics.isInstalled());
    const before = otel_metrics.testPendingCount();
    otel_metrics.recordCreditConsumed(100, .receive, ATTR);
    otel_metrics.observeTokenUsage(7, .input, ATTR);
    otel_metrics.observeCacheReadTokens(3, ATTR);
    otel_metrics.observeInvokeAgentDuration(42, null, ATTR);
    // No sample enqueued because every record path early-returns on !isInstalled().
    try std.testing.expectEqual(before, otel_metrics.testPendingCount());
}

// ── Envelope identity ──────────────────────────────────────────────────────

test "all three signals share one resource identity and the pinned schema url" {
    const alloc = std.testing.allocator;
    var list: std.ArrayList(u8) = .empty;
    defer list.deinit(alloc);
    try otlp_config.appendEnvelopePrefix(&list, alloc, TEST_CFG, "resourceMetrics", "scopeMetrics", "metrics");
    try list.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);

    const body = list.items;
    try std.testing.expect(std.mem.indexOf(u8, body, "\"key\":\"service.namespace\",\"value\":{\"stringValue\":\"agentsfleet\"}") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"schemaUrl\":\"https://opentelemetry.io/schemas/1.43.0\"") != null); // pin test: literal is the contract
    // Absent instance id stays absent — never fabricated.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.RESOURCE_SERVICE_INSTANCE_ID) == null);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

test "a hostile service name cannot break out of the resource envelope" {
    const alloc = std.testing.allocator;
    var cfg = TEST_CFG;
    cfg.service_name = "evil\",\"x\":\"\n\t";
    var list: std.ArrayList(u8) = .empty;
    defer list.deinit(alloc);
    try otlp_config.appendEnvelopePrefix(&list, alloc, cfg, "resourceLogs", "scopeLogs", "logRecords");
    try list.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, list.items, .{});
    parsed.deinit();
}

// ── Serialized wire shapes ─────────────────────────────────────────────────

test "the serialized payload matches the pinned OTLP-JSON fixture" {
    const alloc = std.testing.allocator;

    // Window = [1000, 2000] (delta temporality, one window stamp).
    var s_credit = payload.newSample(.credit_consumed, 0);
    _ = payload.addLabel(&s_credit, semconv.ATTR_CHARGE_TYPE, semconv.ChargeClass.settle.label());
    _ = payload.addLabel(&s_credit, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s_credit, semconv.ATTR_PROVIDER_NAME, PROVIDER);
    _ = payload.addLabel(&s_credit, semconv.ATTR_REQUEST_MODEL, MODEL);

    var s_tokens = payload.newSample(.token_usage, 0);
    _ = payload.addLabel(&s_tokens, semconv.ATTR_OPERATION_NAME, semconv.OPERATION_INVOKE_AGENT);
    _ = payload.addLabel(&s_tokens, semconv.ATTR_TOKEN_TYPE, semconv.TokenType.input.label());
    _ = payload.addLabel(&s_tokens, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s_tokens, semconv.ATTR_PROVIDER_NAME, PROVIDER);
    _ = payload.addLabel(&s_tokens, semconv.ATTR_REQUEST_MODEL, MODEL);

    var s_dur = payload.newSample(.invoke_agent_duration, 0);
    _ = payload.addLabel(&s_dur, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s_dur, semconv.ATTR_REQUEST_MODEL, MODEL);

    // One 37ms observation → the first bucket (≤ 0.1s), index 0. On the
    // client-call table this landed at index 2; the agent table starts a decade
    // later because an agent invocation is not a single provider call.
    var dur_buckets = [_]u64{0} ** payload.N_BUCKETS;
    dur_buckets[0] = 1;
    // One 42-token observation → the (16, 64] bucket, index 3.
    var tok_buckets = [_]u64{0} ** payload.N_BUCKETS;
    tok_buckets[3] = 1;

    // pin test: literal is the contract — these values are what
    // tests/fixtures/telemetry/otlp_metrics.json encodes.
    const series = [_]payload.Series{
        .{ .id = .credit_consumed, .labels = s_credit.labels[0..s_credit.label_count], .sum_value = 123456, .hist_count = 0, .hist_sum = 0, .bucket_counts = &[_]u64{} },
        .{ .id = .token_usage, .labels = s_tokens.labels[0..s_tokens.label_count], .sum_value = 0, .hist_count = 1, .hist_sum = 42, .bucket_counts = &tok_buckets },
        .{ .id = .invoke_agent_duration, .labels = s_dur.labels[0..s_dur.label_count], .sum_value = 0, .hist_count = 1, .hist_sum = 37, .bucket_counts = &dur_buckets },
    };

    // pin test: literal is the contract (window start/now). All three families
    // are delta, so the cumulative process-start stamp never reaches the wire
    // here and the pre-widening fixture stays byte-identical.
    const times = payload.WireTimes{ .window_start_ns = 1000, .process_start_ns = 500, .now_ns = 2000 };
    const envelope = try payload.serializeSeries(alloc, TEST_CFG, &series, times, null);
    const body = envelope.body;
    defer alloc.free(body);

    const fixture = @embedFile("otlp_metrics.json");
    const want = std.mem.trimEnd(u8, fixture, "\n");
    try std.testing.expectEqualStrings(want, body);

    // Belt-and-braces: the serialized body must be valid JSON.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

// Dimension 1.4 — a gauge series serializes in the native OTLP gauge shape,
// with no aggregation temporality and no start time: a level has no history,
// only the instant the flush observed it.
test "test_gauge_serializes_as_gauge" {
    const alloc = std.testing.allocator;
    const series = [_]payload.Series{.{
        .id = .api_in_flight_requests,
        .labels = &[_]payload.Label{},
        .sum_value = 5,
        .hist_count = 0,
        .hist_sum = 0,
        .bucket_counts = &[_]u64{},
    }};
    const times = payload.WireTimes{ .window_start_ns = 1000, .process_start_ns = 500, .now_ns = 2000 };
    const envelope = try payload.serializeSeries(alloc, TEST_CFG, &series, times, null);
    const body = envelope.body;
    defer alloc.free(body);

    try std.testing.expect(std.mem.indexOf(u8, body, "\"gauge\":{\"dataPoints\":[{") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"asInt\":\"5\"") != null);
    // Not a non-monotonic sum, and carrying neither temporality nor start time.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"sum\":{") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "aggregationTemporality") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "startTimeUnixNano") == null);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

// Regression — the five pre-existing cost families keep their names, kinds,
// delta temporality, and label behaviour after the runtime widening.
test "test_cost_families_unchanged_after_widening" {
    const alloc = std.testing.allocator;

    // Declared identity, straight from the registry.
    const dur = families.metaFor(.invoke_agent_duration);
    try std.testing.expectEqualStrings("gen_ai.invoke_agent.duration", dur.name); // pin test: literal is the contract
    try std.testing.expectEqual(families.MetricKind.histogram, dur.kind);
    const tok = families.metaFor(.token_usage);
    try std.testing.expectEqualStrings("agentsfleet.invoke_agent.token.usage", tok.name); // pin test: literal is the contract
    try std.testing.expectEqual(families.MetricKind.histogram, tok.kind);
    const cache = families.metaFor(.cache_read_token_usage);
    try std.testing.expectEqualStrings("agentsfleet.invoke_agent.cache_read.token.usage", cache.name); // pin test: literal is the contract
    try std.testing.expectEqual(families.MetricKind.histogram, cache.kind);
    const credit = families.metaFor(.credit_consumed);
    try std.testing.expectEqualStrings("agentsfleet.billing.credit.consumed", credit.name); // pin test: literal is the contract
    try std.testing.expectEqual(families.MetricKind.sum, credit.kind);
    try std.testing.expect(credit.monotonic);
    const dropped = families.metaFor(.samples_dropped);
    try std.testing.expectEqualStrings("agentsfleet.telemetry.samples_dropped", dropped.name); // pin test: literal is the contract
    try std.testing.expectEqual(families.MetricKind.sum, dropped.kind);

    // Every cost family stays evented (rides the ring) and DELTA on the wire.
    inline for (.{ dur, tok, cache, credit, dropped }) |meta| {
        try std.testing.expect(meta.cost);
        try std.testing.expect(!meta.streamed);
        try std.testing.expectEqual(families.Temporality.delta, meta.temporality);
    }

    // And a serialized window carries the unchanged label behaviour: the
    // charge-class + posture attribution on a recorded credit, as DELTA.
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    otel_metrics.recordCreditConsumed(500, .settle, ATTR);
    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.ExpectedBody;
    defer alloc.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"agentsfleet.billing.credit.consumed\"") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"sum\":{\"aggregationTemporality\":1,\"isMonotonic\":true") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"key\":\"agentsfleet.billing.charge.type\",\"value\":{\"stringValue\":\"settle\"}") != null); // pin test: literal is the contract
    try std.testing.expect(std.mem.indexOf(u8, body, "\"key\":\"agentsfleet.execution.posture\"") != null);
}
