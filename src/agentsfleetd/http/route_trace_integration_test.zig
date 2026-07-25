//! Real request-span coverage over the live server: admission shed, and the
//! standard HTTP server-span shape the pinned conventions define.
//!
//! Both tests drive the REAL router + middleware chain and read the span back
//! off the exporter ring, because the emit site (`emitRequestSpan`) is private
//! to `server.zig` and deliberately reads nothing but the matched route. That
//! privacy is the point: the raw request target holds workspace, fleet, lease,
//! and secret identifiers, so the only honest proof that none of it reaches the
//! wire is to send a request carrying all of it and inspect the serialized span.

const std = @import("std");
const httpz = @import("httpz");
const auth_mw = @import("../auth/middleware/mod.zig");
const metrics_trace = @import("../observability/metrics_trace.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const semconv = @import("../observability/semconv.zig");
const runner_protocol = @import("contract").protocol;
const route_trace = @import("route_trace.zig");
const route_template = @import("route_template.zig");
const harness_mod = @import("test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const TRACE_TEST_CONFIG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "trace-integration",
    .api_key = "trace-integration",
};
const PRIME_FUTURE_SECONDS: i96 = 60;

/// The span name is `{method} {route}` — the pinned HTTP convention's low-
/// cardinality form. Derived from the same enum the server tags the span with
/// so a method-spelling change cannot drift this expectation.
const SPAN_NAME_FMT = "\"name\":\"{s} {s}\"";
const POST_METHOD = @tagName(httpz.Method.POST);
/// `{"key":"…","value":{"stringValue":"…"}}` / `{…{"intValue":"…"}}` — the exact
/// serialized attribute shape, so a needle match proves the key AND its typed
/// value slot, not merely that both bytes appear somewhere in the payload.
const ATTR_STRING_FMT = "{{\"key\":\"{s}\",\"value\":{{\"stringValue\":\"{s}\"}}}}";
const ATTR_INT_FMT = "{{\"key\":\"{s}\",\"value\":{{\"intValue\":\"{d}\"}}}}";
const SPAN_KIND_FMT = "\"kind\":{d}";
const MAX_NEEDLE_BYTES: usize = 256;

// Caller-supplied material the span must never carry: a lease identifier in the
// path, a query string, and an Authorization header. All three are readable on
// the request; none is an input to `emitRequestSpan`.
const SENSITIVE_LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e7c01";
const SENSITIVE_QUERY_KEY = "handoff_token";
const SENSITIVE_QUERY_VALUE = "must-not-reach-the-exporter";
const SENSITIVE_BEARER = "not-a-real-credential";
const UNMATCHED_PATH = "/v1/no-such-route-exists";

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn sendShedReport(h: *TestHarness) !void {
    const response = try h.post(runner_protocol.PATH_RUNNER_REPORTS).rawBody("{}").send();
    defer response.deinit();
    try response.expectStatus(.too_many_requests);
}

fn expectPresent(body: []const u8, needle: []const u8) !void {
    if (std.mem.indexOf(u8, body, needle) == null) {
        std.debug.print("FAIL: request span is missing `{s}`\n", .{needle});
        return error.RequestSpanMissingExpectedContent;
    }
}

fn expectAbsent(body: []const u8, needle: []const u8) !void {
    if (std.mem.indexOf(u8, body, needle) != null) {
        std.debug.print("FAIL: request span carries `{s}`\n", .{needle});
        return error.RequestSpanCarriesRequestTarget;
    }
}

fn stringAttr(buf: []u8, key: []const u8, value: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, ATTR_STRING_FMT, .{ key, value });
}

fn intAttr(buf: []u8, key: []const u8, value: u16) ![]const u8 {
    return std.fmt.bufPrint(buf, ATTR_INT_FMT, .{ key, value });
}

test "integration: test_runner_admission_rejection_is_traced_or_counted" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopRegistry });
    otel_traces.testSetInstalled(TRACE_TEST_CONFIG);
    route_trace.resetForTest();
    metrics_trace.resetForTest();
    defer {
        h.deinit();
        otel_traces.testClear();
        route_trace.resetForTest();
        metrics_trace.resetForTest();
    }
    h.ctx.api_max_in_flight_requests = 0;

    try sendShedReport(h);
    const body = (try otel_traces.testCollect(alloc, TRACE_TEST_CONFIG)) orelse return error.ExpectedRequestSpan;
    defer alloc.free(body);
    // The shed still produces the standard `{method} {route}` name — the 429 is
    // an outcome of the request, not a different kind of observation.
    var name_buf: [MAX_NEEDLE_BYTES]u8 = undefined;
    const shed_name = try std.fmt.bufPrint(&name_buf, SPAN_NAME_FMT, .{ POST_METHOD, runner_protocol.PATH_RUNNER_REPORTS });
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, body, shed_name));

    route_trace.resetForTest();
    metrics_trace.resetForTest();
    const future_second: u64 = @intCast(
        @divTrunc(std.Io.Clock.boot.now(h.ctx.io).toNanoseconds(), std.time.ns_per_s) + PRIME_FUTURE_SECONDS,
    );
    for (0..4) |_| {
        try std.testing.expectEqual(
            route_trace.Decision.emit,
            route_trace.decide(.runner_report, 429, "prime", future_second),
        );
    }

    try sendShedReport(h);
    try std.testing.expectEqual(@as(u64, 1), metrics_trace.snapshot().runner_rejection_budget_total);
    if (try otel_traces.testCollect(alloc, TRACE_TEST_CONFIG)) |unexpected| {
        defer alloc.free(unexpected);
        return error.UnexpectedRequestSpan;
    }
}

test "integration: test_http_server_span_uses_standard_semantics" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopRegistry });
    otel_traces.testClear();
    otel_traces.testSetInstalled(TRACE_TEST_CONFIG);
    route_trace.resetForTest();
    metrics_trace.resetForTest();
    defer {
        h.deinit();
        otel_traces.testClear();
        route_trace.resetForTest();
        metrics_trace.resetForTest();
    }

    // An unmatched path is answered before the trace lifetime begins: there is
    // no route, so there is no template, so no span may claim one.
    const missing = try h.get(UNMATCHED_PATH).send();
    defer missing.deinit();
    try missing.expectStatus(.not_found);
    try std.testing.expectEqual(@as(usize, 0), otel_traces.testPendingCount());

    // A matched request carrying a lease id in the path, a query string, and an
    // Authorization header. The stub runner lookup rejects it — a runner 4xx is
    // admitted by the trace budget deterministically, with no sampling coin flip.
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}/{s}?{s}={s}", .{
        runner_protocol.PATH_RUNNER_LEASES,
        SENSITIVE_LEASE_ID,
        runner_protocol.RUNNER_LEASE_RENEW_SUFFIX,
        SENSITIVE_QUERY_KEY,
        SENSITIVE_QUERY_VALUE,
    });
    defer alloc.free(path);
    const response = try (try (try h.post(path).bearer(SENSITIVE_BEARER)).json("{}")).send();
    defer response.deinit();
    try response.expectStatus(.unauthorized);

    const body = (try otel_traces.testCollect(alloc, TRACE_TEST_CONFIG)) orelse return error.ExpectedRequestSpan;
    defer alloc.free(body);

    // The route template resolved by production code, not a hand-spelled twin.
    const template = route_template.templateFor(.{ .runner_renew = SENSITIVE_LEASE_ID });
    const status: u16 = @intFromEnum(std.http.Status.unauthorized);
    var needle: [MAX_NEEDLE_BYTES]u8 = undefined;

    try expectPresent(body, try std.fmt.bufPrint(&needle, SPAN_NAME_FMT, .{ POST_METHOD, template }));
    // SERVER kind: this process handled an inbound request. INTERNAL would
    // describe the delivery observation instead, and CLIENT would be a lie.
    try expectPresent(body, try std.fmt.bufPrint(&needle, SPAN_KIND_FMT, .{@intFromEnum(otel_traces.SpanKind.server)}));
    try expectPresent(body, try stringAttr(&needle, semconv.ATTR_HTTP_REQUEST_METHOD, POST_METHOD));
    try expectPresent(body, try stringAttr(&needle, semconv.ATTR_HTTP_ROUTE, template));
    // The status is a NUMBER in the pinned conventions. OTLP-JSON spells a
    // 64-bit int as a quoted string, so `"intValue":"401"` is the typed slot —
    // the assertion below pins that it never lands in `stringValue` instead.
    try expectPresent(body, try intAttr(&needle, semconv.ATTR_HTTP_RESPONSE_STATUS_CODE, status));

    var status_text: [MAX_NEEDLE_BYTES]u8 = undefined;
    const status_as_text = try std.fmt.bufPrint(&status_text, "{d}", .{status});
    try expectAbsent(body, try stringAttr(&needle, semconv.ATTR_HTTP_RESPONSE_STATUS_CODE, status_as_text));

    // Nothing the caller supplied survives: not the lease id the path carried,
    // not the query string, not the bearer credential.
    for ([_][]const u8{
        SENSITIVE_LEASE_ID,
        SENSITIVE_QUERY_KEY,
        SENSITIVE_QUERY_VALUE,
        SENSITIVE_BEARER,
    }) |forbidden| {
        try expectAbsent(body, forbidden);
    }
}
