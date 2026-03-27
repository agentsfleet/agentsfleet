//! HTTP metrics and health server for daemon mode.
//!
//! Exports:
//!   - `metricsServerThread` — thread entry point; starts the httpz listener.
//!   - `stopMetricsServer`   — signal the metrics server to stop.
//!   - `appendMetric`        — write a single Prometheus text-format metric block.
//!   - `renderDaemonMetrics` — render full Prometheus exposition with daemon counters.
//!
//! Ownership:
//!   - `renderDaemonMetrics` returns a caller-owned `[]u8`; call `alloc.free` on it.
//!   - `metricsServerThread` does not allocate retained memory.

const std = @import("std");
const httpz = @import("httpz");
const metrics = @import("../../observability/metrics.zig");
const state_mod = @import("state.zig");

const log = std.log.scoped(.reconcile);

/// httpz handler struct for the daemon metrics server.
const DaemonApp = struct {
    pub fn handle(_: DaemonApp, req: *httpz.Request, res: *httpz.Response) void {
        const path = req.url.path;
        const s = state_mod.g_daemon_state orelse {
            res.status = @intFromEnum(std.http.Status.service_unavailable);
            res.body = "";
            return;
        };

        if (std.mem.eql(u8, path, "/healthz")) {
            const healthy = state_mod.daemonHealthy(s, std.time.milliTimestamp());
            if (healthy) {
                res.status = @intFromEnum(std.http.Status.ok);
                res.body = "{\"status\":\"ok\",\"service\":\"reconcile\"}";
            } else {
                res.status = @intFromEnum(std.http.Status.service_unavailable);
                res.body = "{\"status\":\"degraded\",\"service\":\"reconcile\"}";
            }
            return;
        }

        if (std.mem.eql(u8, path, "/metrics")) {
            // Use the request arena so the body stays valid until httpz sends the response.
            const body = renderDaemonMetrics(req.arena, s) catch {
                res.status = @intFromEnum(std.http.Status.internal_server_error);
                res.body = "";
                return;
            };
            res.status = @intFromEnum(std.http.Status.ok);
            res.header("content-type", "text/plain; charset=utf-8");
            res.body = body;
            return;
        }

        res.status = @intFromEnum(std.http.Status.not_found);
        res.body = "{\"error\":\"NOT_FOUND\"}";
    }

    pub fn uncaughtError(_: DaemonApp, _: *httpz.Request, res: *httpz.Response, _: anyerror) void {
        res.status = 500;
        res.body = "";
    }
};

/// Module-level server pointer for cross-thread stop.
var g_daemon_server: ?*httpz.Server(DaemonApp) = null;

pub fn metricsServerThread(port: u16) !void {
    var server = try httpz.Server(DaemonApp).init(std.heap.page_allocator, .{
        .address = .{ .ip = .{ .host = "::", .port = port } },
        .workers = .{
            .max_conn = 128,
        },
        .request = .{
            .max_body_size = 64 * 1024,
        },
    }, .{});
    defer server.deinit();

    g_daemon_server = &server;
    defer g_daemon_server = null;

    log.info("reconcile.metrics_listening port={d}", .{port});
    try server.listen();
}

pub fn stopMetricsServer() void {
    if (g_daemon_server) |s| s.stop();
}

pub fn appendMetric(writer: anytype, name: []const u8, metric_type: []const u8, help: []const u8, value: anytype) !void {
    try writer.print("# HELP {s} {s}\n", .{ name, help });
    try writer.print("# TYPE {s} {s}\n", .{ name, metric_type });
    try writer.print("{s} {d}\n", .{ name, value });
}

pub fn renderDaemonMetrics(alloc: std.mem.Allocator, s: *state_mod.DaemonState) ![]u8 {
    const base = try metrics.renderPrometheus(
        alloc,
        s.running.load(.acquire),
        null,
        null,
    );
    defer alloc.free(base);

    var list: std.ArrayList(u8) = .{};
    errdefer list.deinit(alloc);
    try list.appendSlice(alloc, base);

    const writer = list.writer(alloc);
    try appendMetric(writer, "zombied_reconcile_last_attempt_timestamp_ms", "gauge", "Last reconcile attempt timestamp in unix milliseconds.", s.last_attempt_ms.load(.acquire));
    try appendMetric(writer, "zombied_reconcile_last_success_timestamp_ms", "gauge", "Last successful reconcile timestamp in unix milliseconds.", s.last_success_ms.load(.acquire));
    try appendMetric(writer, "zombied_reconcile_last_dead_lettered", "gauge", "Rows dead-lettered by the latest reconcile tick.", s.last_dead_lettered.load(.acquire));
    try appendMetric(writer, "zombied_reconcile_total_ticks", "counter", "Total reconcile ticks attempted in daemon mode.", s.total_ticks.load(.acquire));
    try appendMetric(writer, "zombied_reconcile_consecutive_failures", "gauge", "Current consecutive reconcile tick failure streak.", s.consecutive_failures.load(.acquire));

    return list.toOwnedSlice(alloc);
}
