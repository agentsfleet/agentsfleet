//! Sample-shape and attribution-budget behaviour for the metrics exporter —
//! split from otel_metrics_flush_test.zig for the file-length cap. Registered
//! from tests.zig.

const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const payload = @import("otel_metrics_payload.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const semconv = @import("semconv.zig");
const otlp_config = @import("otlp/config.zig");
const health = @import("metrics_otel.zig");

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

test "test_attribution_budget_reopens_each_flush_window" {
    const alloc = std.testing.allocator;
    cardinality.reset();
    defer cardinality.reset();
    health.resetForTest();
    defer health.resetForTest();
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();

    // Spend the whole budget on pairs that will go idle after this window.
    var buf: [32]u8 = undefined;
    var i: usize = 0;
    while (i < cardinality.ATTRIBUTION_CAP) : (i += 1) {
        const model = try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{i});
        try std.testing.expect(cardinality.admitModel(PROVIDER, model));
    }
    const first = try std.fmt.bufPrint(&buf, MODEL_FIXTURE_FMT, .{0});
    otel_metrics.recordCreditConsumed(11, .receive, .{ .posture = POSTURE, .provider = PROVIDER, .model = first });

    // Inside one window the budget is a real ceiling.
    try std.testing.expect(!cardinality.admitModel(PROVIDER, "arrives-later"));

    // The flush drains exactly the samples those admissions governed, so the
    // window they were charged against is now closed.
    if (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) |body| alloc.free(body);

    // The ceiling is per-window (the Aggregator is rebuilt each flush), so the
    // next window starts empty. A model arriving now is attributed rather than
    // starved by pairs that stopped spending series in the previous window.
    try std.testing.expectEqual(@as(usize, 0), cardinality.trackedCount());
    try std.testing.expect(cardinality.admitModel(PROVIDER, "arrives-later"));

    const s = otel_metrics.testPop();
    try std.testing.expect(s == null);
}
