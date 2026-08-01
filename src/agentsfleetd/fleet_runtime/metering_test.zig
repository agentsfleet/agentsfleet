// Unit tests for src/agentsfleetd/fleet_runtime/metering.zig — the pure
// surfaces only. The DB-backed credit-pool tests live in
// metering_integration_test.zig (integration root), where the canonical lanes
// provide a live database.

const std = @import("std");

const metering = @import("metering.zig");

const ALLOC = std.testing.allocator;

test "DebitOutcome variants compile and pattern-match" {
    const r1: metering.DebitOutcome = .{ .deducted = 7 };
    const r2: metering.DebitOutcome = .{ .exhausted = {} };
    const r3: metering.DebitOutcome = .{ .missing_tenant_billing = {} };
    const r4: metering.DebitOutcome = .{ .db_error = {} };

    try std.testing.expectEqual(@as(i64, 7), switch (r1) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });
    switch (r2) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }
    switch (r3) {
        .missing_tenant_billing => {},
        else => return error.TestExpectedEqual,
    }
    switch (r4) {
        .db_error => {},
        else => return error.TestExpectedEqual,
    }
}

// ── The `fleet.delivery` span ────────────────────────────────────────────
//
// The span is a CUSTOM control-plane observation, not a claimed runner trace:
// the runner produces no span and propagates no trace context. These tests pin
// two things the schema depends on — the attribute set is exactly the standard
// GenAI keys plus namespaced product correlations, and prompt/response content
// never reaches it. No database: the emit path is observability only.

const otel_traces = @import("../observability/otel_traces.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const semconv = @import("../observability/semconv.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const SPAN_TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};

const SPAN_TENANT = "0195b4ba-8d3a-7f13-8abc-fa0500000099";
const SPAN_EPOCH_MS: i64 = 1_700_000_000_000;
/// A believable run length for the span window; any positive value works.
const SPAN_WALL_MS: u64 = 1_000;

fn deliveryContext(provider: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = "0195b4ba-8d3a-7f13-8abc-aa0500000099",
        .fleet_id = "0195b4ba-8d3a-7f13-8abc-bb0500000099",
        .event_id = "0195b4ba-8d3a-7f13-8abc-cc0500000099",
        .posture = tenant_provider.Mode.platform,
        .provider = provider,
        .model = "claude-opus-4-8",
    };
}

test "test_delivery_span_uses_semantic_attributes_without_runner_claim" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 80, 30, 5_000, SPAN_EPOCH_MS);
    try std.testing.expectEqual(@as(usize, 1), otel_traces.testPendingCount());

    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    var doc = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
    defer doc.deinit();

    // A custom control-plane observation, not a client span: INTERNAL kind,
    // product span name. CLIENT would assert this process called the provider.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"" ++ semconv.SPAN_FLEET_DELIVERY ++ "\"") != null);
    var kind_buf: [32]u8 = undefined;
    const internal_kind = try std.fmt.bufPrint(&kind_buf, "\"kind\":{d}", .{@intFromEnum(otel_traces.SpanKind.internal)});
    try std.testing.expect(std.mem.indexOf(u8, body, internal_kind) != null);

    // Standard where the fact is standard.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_OPERATION_NAME) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"stringValue\":\"" ++ semconv.OPERATION_INVOKE_AGENT ++ "\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_AGENT_ID) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_PROVIDER_NAME) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_REQUEST_MODEL) != null);

    // Usage counts land in the TYPED `intValue` slot, never `stringValue`, so a
    // backend aggregates them instead of treating tokens as an opaque label.
    // (OTLP-JSON still spells a 64-bit int as a quoted string — that is the
    // wire encoding for the int type, not a string-typed attribute.)
    try std.testing.expect(std.mem.indexOf(u8, body, "\"intValue\":\"80\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"intValue\":\"30\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"stringValue\":\"80\"") == null);

    // Correlation identity is allowed on a SPAN (bounded per-event record) even
    // though it is forbidden on a METRIC (unbounded series).
    for ([_][]const u8{ semconv.ATTR_WORKSPACE_ID, semconv.ATTR_TENANT_ID, semconv.ATTR_EVENT_ID }) |key| {
        try std.testing.expect(std.mem.indexOf(u8, body, key) != null);
    }
}

test "the delivery span carries no prompt, response, or credential material" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, SPAN_EPOCH_MS);
    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    // The emit signature cannot even receive content — this pins that a future
    // edit does not quietly widen it. Names come from the pinned GenAI content
    // conventions this process deliberately does not implement.
    for ([_][]const u8{
        "gen_ai.prompt",
        "gen_ai.completion",
        "gen_ai.input.messages",
        "gen_ai.output.messages",
        "response_text",
        "api_key",
        "authorization",
    }) |forbidden| {
        if (std.mem.indexOf(u8, body, forbidden) != null) {
            std.debug.print("FAIL: delivery span carries `{s}`\n", .{forbidden});
            return error.DeliverySpanCarriesContent;
        }
    }
}

test "an unknown provider omits gen_ai.provider.name rather than inventing one" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    // "megacorp-ai" maps to no well-known name. Exporting it under the standard
    // key would publish a private spelling as though upstream defined it.
    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("megacorp-ai"), 1, 1, SPAN_WALL_MS, SPAN_EPOCH_MS);
    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_PROVIDER_NAME) == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "megacorp-ai") == null);
    // The measurement itself survives the omission.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_REQUEST_MODEL) != null);
}

test "a non-positive epoch emits no span rather than a corrupt timeline" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, 0);
    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, -1);
    try std.testing.expectEqual(@as(usize, 0), otel_traces.testPendingCount());
}
