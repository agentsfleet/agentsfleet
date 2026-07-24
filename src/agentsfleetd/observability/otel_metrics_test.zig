const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const semconv = @import("semconv.zig");
const otlp_config = @import("otlp/config.zig");
const health = @import("metrics_otel.zig");

const Ring = otel_metrics.TestRing;
const BUFFER_CAPACITY = otel_metrics.TEST_BUFFER_CAPACITY;

// Fixture identity. The posture is a real resolver label, not an invented one,
// so the attribute values these tests pin are the values production emits.
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

fn sampleWithLabels(id: payload.MetricId, value: i64) payload.Sample {
    var s = payload.newSample(id, value);
    _ = payload.addLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
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

fn omissionCount(attribute: health.OmittedAttribute, reason: health.OmissionReason) u64 {
    return health.snapshot().attribute_omitted[@intFromEnum(attribute)][@intFromEnum(reason)];
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
    const duration = payload.metaFor(.invoke_agent_duration).bounds;
    // Milliseconds in, because that is what the runner reports.
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(5, duration)); // <= first bound (100ms)
    try std.testing.expectEqual(@as(usize, 0), payload.bucketIndex(100, duration)); // inclusive upper edge
    try std.testing.expectEqual(@as(usize, 2), payload.bucketIndex(370, duration)); // (200, 400]
    // Past the last bound → the trailing +Inf bucket.
    try std.testing.expectEqual(@as(usize, duration.len), payload.bucketIndex(999_999, duration));

    const tokens = payload.metaFor(.token_usage).bounds;
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

// ── Dimension 2.2 ──────────────────────────────────────────────────────────

test "test_invoke_agent_token_usage_never_double_counts_cache" {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    cardinality.reset();
    defer cardinality.reset();

    // 80 regular input + 20 cached input + 30 output. The exported input
    // direction is 100 (regular + cached), cached is reported once as a subset,
    // and nothing ever sums to 130.
    otel_metrics.recordRunSettlement(1000, 80, 20, 30, 100, null, ATTR); // pin test: literal is the contract

    const credit = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.credit_consumed, credit.id);
    try std.testing.expectEqualStrings(
        semconv.ChargeClass.settle.label(),
        findLabel(&credit, semconv.ATTR_CHARGE_TYPE).?,
    );

    const input = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.token_usage, input.id);
    try std.testing.expectEqual(@as(i64, 100), input.value);
    try std.testing.expectEqualStrings(semconv.TokenType.input.label(), findLabel(&input, semconv.ATTR_TOKEN_TYPE).?);
    try std.testing.expectEqualStrings(semconv.OPERATION_INVOKE_AGENT, findLabel(&input, semconv.ATTR_OPERATION_NAME).?);

    const output = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.token_usage, output.id);
    try std.testing.expectEqual(@as(i64, 30), output.value);
    try std.testing.expectEqualStrings(semconv.TokenType.output.label(), findLabel(&output, semconv.ATTR_TOKEN_TYPE).?);

    // The cached subset rides its own metric, so it can never be summed as a
    // third token direction alongside input and output.
    const cached = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.cache_read_token_usage, cached.id);
    try std.testing.expectEqual(@as(i64, 20), cached.value);
    try std.testing.expect(findLabel(&cached, semconv.ATTR_TOKEN_TYPE) == null);

    const duration = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.invoke_agent_duration, duration.id);
    try std.testing.expectEqual(@as(i64, 100), duration.value);
    // A clean run carries no error.type at all.
    try std.testing.expect(findLabel(&duration, semconv.ATTR_ERROR_TYPE) == null);

    try std.testing.expect(otel_metrics.testPop() == null);
}

test "zero-valued token directions emit no misleading series" {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    cardinality.reset();
    defer cardinality.reset();

    // No cached input and no output: only the input direction, the credit, and
    // the duration should exist. A zero observation would otherwise create a
    // series implying the run produced measured-zero output.
    otel_metrics.recordRunSettlement(0, 10, 0, 0, 5, semconv.ERROR_TYPE_FLEET_ERROR, ATTR);

    const input = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.token_usage, input.id);
    try std.testing.expectEqual(@as(i64, 10), input.value);

    const duration = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(payload.MetricId.invoke_agent_duration, duration.id);
    try std.testing.expectEqualStrings(
        semconv.ERROR_TYPE_FLEET_ERROR,
        findLabel(&duration, semconv.ATTR_ERROR_TYPE).?,
    );

    // A zero credit debit is not a debit, and the cached subset was zero.
    try std.testing.expect(otel_metrics.testPop() == null);
}

// ── Dimension 2.3 ──────────────────────────────────────────────────────────

test "test_metric_attribute_cardinality_is_bounded_and_visible" {
    cardinality.reset();
    defer cardinality.reset();
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // Fill the derived budget with distinct (provider, model) pairs.
    var buf: [32]u8 = undefined;
    var i: usize = 0;
    while (i < cardinality.ATTRIBUTION_CAP) : (i += 1) {
        const model = try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{i});
        try std.testing.expect(cardinality.admitModel(PROVIDER, model));
    }
    try std.testing.expectEqual(cardinality.ATTRIBUTION_CAP, cardinality.trackedCount());

    // The next pair loses only the model attribute; the measurement survives.
    otel_metrics.recordCreditConsumed(7, .receive, .{ .posture = POSTURE, .provider = PROVIDER, .model = "model-past-cap" });
    const capped = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expectEqual(@as(i64, 7), capped.value);
    try std.testing.expect(findLabel(&capped, semconv.ATTR_REQUEST_MODEL) == null);
    // Provider is bounded by the well-known allowlist, so it is still exported.
    try std.testing.expectEqualStrings(PROVIDER, findLabel(&capped, semconv.ATTR_PROVIDER_NAME).?);
    try std.testing.expectEqual(@as(u64, 1), omissionCount(.request_model, .budget_exhausted));

    // An already-admitted pair keeps its attribution, and the budget never grows.
    const seen = try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{0});
    try std.testing.expect(cardinality.admitModel(PROVIDER, seen));
    try std.testing.expectEqual(cardinality.ATTRIBUTION_CAP, cardinality.trackedCount());
}

test "an unmapped provider is omitted rather than exported as a standard value" {
    cardinality.reset();
    defer cardinality.reset();
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    otel_metrics.recordCreditConsumed(3, .receive, .{ .posture = POSTURE, .provider = "some-internal-gateway", .model = MODEL });

    const s = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expect(findLabel(&s, semconv.ATTR_PROVIDER_NAME) == null);
    // The measurement and the model attribution both survive the omission.
    try std.testing.expectEqual(@as(i64, 3), s.value);
    try std.testing.expectEqualStrings(MODEL, findLabel(&s, semconv.ATTR_REQUEST_MODEL).?);
    try std.testing.expectEqual(@as(u64, 1), omissionCount(.provider_name, .unmapped_provider));
}

test "an over-long model is omitted rather than truncated into a different model" {
    cardinality.reset();
    defer cardinality.reset();
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    const huge_model = "m" ** (payload.MAX_LABEL_VAL + 1);
    otel_metrics.recordCreditConsumed(3, .receive, .{ .posture = POSTURE, .provider = PROVIDER, .model = huge_model });

    const s = otel_metrics.testPop() orelse return error.NoSampleEnqueued;
    try std.testing.expect(findLabel(&s, semconv.ATTR_REQUEST_MODEL) == null);
    try std.testing.expectEqual(@as(u64, 1), omissionCount(.request_model, .value_too_long));
    // Rejected by the budget guard too, so an unbounded value cannot consume a slot.
    try std.testing.expectEqual(@as(usize, 0), cardinality.trackedCount());
}

test "concurrent attribution stays inside the derived budget" {
    cardinality.reset();
    defer cardinality.reset();

    const Worker = struct {
        fn run(offset: usize) void {
            var buf: [32]u8 = undefined;
            var n: usize = 0;
            while (n < cardinality.ATTRIBUTION_CAP) : (n += 1) {
                const model = std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{offset * 1000 + n}) catch return;
                _ = cardinality.admitModel(PROVIDER, model);
            }
        }
    };
    var threads: [4]std.Thread = undefined;
    for (&threads, 0..) |*t, idx| t.* = try std.Thread.spawn(.{}, Worker.run, .{idx});
    for (&threads) |t| t.join();

    // Four threads each raced far more pairs than the budget allows; the guard
    // admits exactly the cap and never overruns its fixed storage.
    try std.testing.expectEqual(cardinality.ATTRIBUTION_CAP, cardinality.trackedCount());
}

// ── Exporter lifecycle + flush behaviour ───────────────────────────────────

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

    // pin test: literal is the contract (window start/now)
    const body = try payload.serializeSeries(alloc, TEST_CFG, &series, 1000, 2000);
    defer alloc.free(body);

    const fixture = @embedFile("otlp_metrics.json");
    const want = std.mem.trimEnd(u8, fixture, "\n");
    try std.testing.expectEqualStrings(want, body);

    // Belt-and-braces: the serialized body must be valid JSON.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
}

test "install then uninstall completes with no hang" {
    _ = otel_metrics.install(@import("common").globalIo(), TEST_CFG);
    try std.testing.expect(otel_metrics.isInstalled());
    // Empty ring → the flush thread never POSTs; uninstall wakes the tick sleep
    // and joins within one SLEEP_TICK_MS, leaving the exporter disabled.
    otel_metrics.uninstall();
    try std.testing.expect(!otel_metrics.isInstalled());
}

test "a flush drains and aggregates the window; the next is empty" {
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

    // Window reset: the ring is drained, so the next flush is empty (delta).
    try std.testing.expectEqual(@as(usize, 0), otel_metrics.testPendingCount());
    const body2 = try otel_metrics.testCollectOnce(alloc, TEST_CFG);
    try std.testing.expect(body2 == null);
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

test "the series cap drops surface separately from ring-full drops" {
    const alloc = std.testing.allocator;
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // MAX_SERIES+1 distinct label sets. The model attribute is budget-capped, so
    // vary the posture-free dimension the aggregator actually keys on by
    // building samples directly — this isolates the series-cap drop from both
    // the ring-full drop and the attribution budget.
    var buf: [32]u8 = undefined;
    var i: usize = 0;
    while (i < aggregate.MAX_SERIES + 1) : (i += 1) {
        var s = payload.newSample(.credit_consumed, 1);
        _ = payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{i}));
        otel_metrics.testPush(s);
    }
    const body = (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.METRIC_SAMPLES_DROPPED) != null);
    const snapshot = health.snapshot();
    try std.testing.expectEqual(
        @as(u64, 1),
        snapshot.discarded[@intFromEnum(health.Signal.metrics)][@intFromEnum(health.DiscardReason.aggregate_cap)],
    );
}
