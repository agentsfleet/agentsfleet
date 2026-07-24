//! Real admission-shed coverage for the request trace lifetime.

const std = @import("std");
const auth_mw = @import("../auth/middleware/mod.zig");
const metrics_trace = @import("../observability/metrics_trace.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const runner_protocol = @import("contract").protocol;
const route_trace = @import("route_trace.zig");
const harness_mod = @import("test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const TRACE_TEST_CONFIG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "trace-integration",
    .api_key = "trace-integration",
};
const PRIME_FUTURE_SECONDS: i96 = 60;

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn sendShedReport(h: *TestHarness) !void {
    const response = try h.post(runner_protocol.PATH_RUNNER_REPORTS).rawBody("{}").send();
    defer response.deinit();
    try response.expectStatus(.too_many_requests);
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
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, body, "\"name\":\"http.request\""));
    try std.testing.expect(std.mem.indexOf(u8, body, runner_protocol.PATH_RUNNER_REPORTS) != null);

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
