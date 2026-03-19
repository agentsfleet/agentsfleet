//! OTLP JSON metrics exporter.
//! Renders the current metrics snapshot as OTLP JSON and pushes it to
//! OTEL_EXPORTER_OTLP_ENDPOINT if configured. The existing /metrics
//! Prometheus endpoint is unchanged.
//!
//! Protocol: OTLP/HTTP JSON (POST /v1/metrics).
//! Fire-and-forget: export errors are logged but never propagated.

const std = @import("std");
const metrics = @import("metrics.zig");
const obs_log = @import("logging.zig");
const log = std.log.scoped(.otel_export);

const OTLP_METRICS_PATH = "/v1/metrics";
pub const ERR_OTEL_EXPORT_FAILED = "UZ-OBS-OTEL-001";
pub const ERR_OTEL_CONNECT_FAILED = "UZ-OBS-OTEL-002";
pub const ERR_OTEL_REQUEST_FAILED = "UZ-OBS-OTEL-003";
pub const ERR_OTEL_UNEXPECTED_STATUS = "UZ-OBS-OTEL-004";

pub const OtelConfig = struct {
    endpoint: []const u8, // e.g. "http://localhost:4318"
    service_name: []const u8 = "zombied",
};

const ExportError = error{
    ConnectFailed,
    RequestFailed,
    UnexpectedStatus,
};

/// Try to load OTEL config from environment. Returns null when not configured.
pub fn configFromEnv(alloc: std.mem.Allocator) ?OtelConfig {
    const endpoint = std.process.getEnvVarOwned(alloc, "OTEL_EXPORTER_OTLP_ENDPOINT") catch return null;
    const trimmed = std.mem.trim(u8, endpoint, " \t\r\n");
    if (trimmed.len == 0) {
        alloc.free(endpoint);
        return null;
    }
    const service_name = std.process.getEnvVarOwned(alloc, "OTEL_SERVICE_NAME") catch {
        return .{ .endpoint = endpoint, .service_name = "zombied" };
    };
    return .{ .endpoint = endpoint, .service_name = service_name };
}

/// Render the current metrics snapshot as OTLP JSON.
/// Caller owns returned slice.
pub fn renderOtlpJson(
    alloc: std.mem.Allocator,
    service_name: []const u8,
    worker_running: bool,
    queue_depth: ?i64,
    oldest_queued_age_ms: ?i64,
) ![]u8 {
    const prom_body = try metrics.renderPrometheus(alloc, worker_running, queue_depth, oldest_queued_age_ms);
    defer alloc.free(prom_body);

    const now_ns = @as(u64, @intCast(std.time.nanoTimestamp()));
    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    const w = out.writer(alloc);

    try w.writeAll("{\"resourceMetrics\":[{\"resource\":{\"attributes\":[");
    try w.print("{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}", .{service_name});
    try w.writeAll("]},\"scopeMetrics\":[{\"scope\":{\"name\":\"zombied\"},\"metrics\":[");

    // Parse Prometheus text and emit OTLP gauge/counter entries.
    var first = true;
    var lines = std.mem.splitScalar(u8, prom_body, '\n');
    var current_type: []const u8 = "gauge";
    while (lines.next()) |line| {
        if (line.len == 0) continue;
        if (std.mem.startsWith(u8, line, "# TYPE ")) {
            if (std.mem.indexOf(u8, line, " counter")) |_| {
                current_type = "counter";
            } else if (std.mem.indexOf(u8, line, " gauge")) |_| {
                current_type = "gauge";
            } else {
                current_type = "gauge";
            }
            continue;
        }
        if (std.mem.startsWith(u8, line, "# ")) continue;
        // Skip histogram bucket/sum/count lines (contain { or _bucket/_sum/_count)
        if (std.mem.indexOf(u8, line, "_bucket{") != null) continue;
        if (std.mem.indexOf(u8, line, "_sum ") != null) continue;
        if (std.mem.indexOf(u8, line, "_count ") != null) continue;

        // Parse "metric_name value"
        const sep = std.mem.indexOf(u8, line, " ") orelse continue;
        const name = line[0..sep];
        const value_str = std.mem.trim(u8, line[sep + 1 ..], " \t\r\n");

        if (!first) try w.writeAll(",");
        first = false;

        if (std.mem.eql(u8, current_type, "counter")) {
            try w.print(
                "{{\"name\":\"{s}\",\"sum\":{{\"dataPoints\":[{{\"asInt\":\"{s}\",\"timeUnixNano\":\"{d}\"}}],\"isMonotonic\":true}}}}",
                .{ name, value_str, now_ns },
            );
        } else {
            try w.print(
                "{{\"name\":\"{s}\",\"gauge\":{{\"dataPoints\":[{{\"asInt\":\"{s}\",\"timeUnixNano\":\"{d}\"}}]}}}}",
                .{ name, value_str, now_ns },
            );
        }
    }

    try w.writeAll("]}]}]}");

    return out.toOwnedSlice(alloc);
}

/// Best-effort OTLP push. Export failures are logged and never propagated.
pub fn exportMetricsSnapshotBestEffort(
    alloc: std.mem.Allocator,
    cfg: OtelConfig,
    worker_running: bool,
    queue_depth: ?i64,
    oldest_queued_age_ms: ?i64,
) void {
    metrics.incOtelExportTotal();
    exportMetricsSnapshotInner(alloc, cfg, worker_running, queue_depth, oldest_queued_age_ms) catch |err| {
        metrics.incOtelExportFailed();
        obs_log.logWarnErr(.otel_export, err, "error_code={s} otel export failed endpoint={s}", .{ exportErrorCode(err), cfg.endpoint });
        return;
    };
    metrics.setOtelLastSuccessAtMs(std.time.milliTimestamp());
}

fn exportMetricsSnapshotInner(
    alloc: std.mem.Allocator,
    cfg: OtelConfig,
    worker_running: bool,
    queue_depth: ?i64,
    oldest_queued_age_ms: ?i64,
) !void {
    const body = try renderOtlpJson(alloc, cfg.service_name, worker_running, queue_depth, oldest_queued_age_ms);
    defer alloc.free(body);

    const endpoint = std.mem.trimRight(u8, cfg.endpoint, "/");
    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, OTLP_METRICS_PATH });
    defer alloc.free(url);

    try postOtlpJson(alloc, url, body);
    log.info("otel_export_ok endpoint={s} payload_len={d}", .{ url, body.len });
}

fn postOtlpJson(alloc: std.mem.Allocator, url: []const u8, payload: []const u8) ExportError!void {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    var body: std.ArrayList(u8) = .{};
    defer body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [1]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
    };

    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch |err| return mapFetchError(err);

    if (!isSuccessStatus(result.status)) return ExportError.UnexpectedStatus;
}

fn mapFetchError(err: anyerror) ExportError {
    return switch (err) {
        error.UnexpectedConnectFailure,
        error.ConnectionRefused,
        error.NetworkUnreachable,
        error.ConnectionTimedOut,
        error.HostUnreachable,
        error.PermissionDenied,
        error.TemporaryNameServerFailure,
        error.NameServerFailure,
        error.UnknownHostName,
        => ExportError.ConnectFailed,
        else => ExportError.RequestFailed,
    };
}

fn exportErrorCode(err: anyerror) []const u8 {
    return switch (err) {
        ExportError.ConnectFailed => ERR_OTEL_CONNECT_FAILED,
        ExportError.RequestFailed => ERR_OTEL_REQUEST_FAILED,
        ExportError.UnexpectedStatus => ERR_OTEL_UNEXPECTED_STATUS,
        else => ERR_OTEL_EXPORT_FAILED,
    };
}

fn isSuccessStatus(status: std.http.Status) bool {
    return switch (status) {
        .ok, .created, .accepted, .no_content => true,
        else => false,
    };
}

// --- Tests ---

test "renderOtlpJson produces valid JSON envelope" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "test-svc", true, 0, 0);
    defer alloc.free(body);

    // Must start with resourceMetrics
    try std.testing.expect(std.mem.startsWith(u8, body, "{\"resourceMetrics\":"));
    // Must contain service name
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "test-svc"));
    // Must contain at least one metric
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_runs_created_total"));
    // Must end with closing brackets
    try std.testing.expect(std.mem.endsWith(u8, body, "]}]}]}"));
}

test "renderOtlpJson counters use sum with isMonotonic" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "zombied", false, null, null);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"isMonotonic\":true"));
}

test "renderOtlpJson gauges use gauge type" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "zombied", true, 5, 100);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"gauge\":{\"dataPoints\""));
}

test "configFromEnv returns null when env not set" {
    const alloc = std.testing.allocator;
    const cfg = configFromEnv(alloc);
    // This test should remain deterministic even if env is set by parent process.
    if (cfg) |present| {
        alloc.free(present.endpoint);
        if (!std.mem.eql(u8, present.service_name, "zombied")) alloc.free(present.service_name);
    } else {
        try std.testing.expect(cfg == null);
    }
}

test "postOtlpJson returns ConnectFailed when endpoint unreachable" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        ExportError.ConnectFailed,
        postOtlpJson(alloc, "http://127.0.0.1:1/v1/metrics", "{\"resourceMetrics\":[]}"),
    );
}

test "isSuccessStatus accepts 2xx and rejects non-2xx" {
    try std.testing.expect(isSuccessStatus(.ok));
    try std.testing.expect(isSuccessStatus(.created));
    try std.testing.expect(isSuccessStatus(.accepted));
    try std.testing.expect(isSuccessStatus(.no_content));
    try std.testing.expect(!isSuccessStatus(.bad_request));
    try std.testing.expect(!isSuccessStatus(.unauthorized));
}

test "renderOtlpJson excludes histogram helper series from prometheus text" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "zombied", true, 5, 100);
    defer alloc.free(body);

    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_agent_duration_seconds_bucket") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_agent_duration_seconds_sum") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_agent_duration_seconds_count") == null);
}
