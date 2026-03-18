//! HTTP metrics and health server for daemon mode.
//!
//! Exports:
//!   - `daemonDispatch`      — zap request handler for /healthz and /metrics.
//!   - `metricsServerThread` — thread entry point; starts the zap listener.
//!   - `appendMetric`        — write a single Prometheus text-format metric block.
//!   - `renderDaemonMetrics` — render full Prometheus exposition with daemon counters.
//!
//! Ownership:
//!   - `renderDaemonMetrics` returns a caller-owned `[]u8`; call `alloc.free` on it.
//!   - `daemonDispatch` and `metricsServerThread` do not allocate retained memory.

const std = @import("std");
const zap = @import("zap");
const metrics = @import("../../observability/metrics.zig");
const state_mod = @import("state.zig");

const log = std.log.scoped(.reconcile);

pub fn daemonDispatch(r: zap.Request) !void {
    const path = r.path orelse {
        r.setStatus(.bad_request);
        r.sendBody("") catch {};
        return;
    };
    const s = state_mod.g_daemon_state orelse {
        r.setStatus(.service_unavailable);
        r.sendBody("") catch {};
        return;
    };

    if (std.mem.eql(u8, path, "/healthz")) {
        const healthy = state_mod.daemonHealthy(s, std.time.milliTimestamp());
        if (healthy) {
            r.setStatus(.ok);
            r.sendBody("{\"status\":\"ok\",\"service\":\"reconcile\"}") catch {};
        } else {
            r.setStatus(.service_unavailable);
            r.sendBody("{\"status\":\"degraded\",\"service\":\"reconcile\"}") catch {};
        }
        return;
    }

    if (std.mem.eql(u8, path, "/metrics")) {
        const body = renderDaemonMetrics(s.alloc, s) catch {
            r.setStatus(.internal_server_error);
            r.sendBody("") catch {};
            return;
        };
        defer s.alloc.free(body);
        r.setStatus(.ok);
        r.setContentType(.TEXT) catch {};
        r.sendBody(body) catch {};
        return;
    }

    r.setStatus(.not_found);
    r.sendBody("{\"error\":\"NOT_FOUND\"}") catch {};
}

pub fn metricsServerThread(port: u16) !void {
    var listener = zap.HttpListener.init(.{
        .port = port,
        .on_request = daemonDispatch,
        .log = false,
        .max_clients = 128,
        .max_body_size = 64 * 1024,
    });
    try listener.listen();
    log.info("reconcile metrics listening on 0.0.0.0:{d}", .{port});
    zap.start(.{
        .threads = 1,
        .workers = 1,
    });
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
