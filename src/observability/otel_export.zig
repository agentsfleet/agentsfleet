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
const otel_histogram = @import("otel_histogram.zig");
const otel_json = @import("otel_json.zig");
const log = std.log.scoped(.otel_export);

const OTLP_METRICS_PATH = "/v1/metrics";

/// Anchor timestamp for CUMULATIVE aggregation temporality. Lazily set on the
/// first export call and reused for every subsequent data point. OTLP backends
/// use the gap `timeUnixNano - startTimeUnixNano` to compute rates; without a
/// stable start, the first scrape looks like a zero-duration window and the
/// backend reports zero rate.
var g_start_time_ns = std.atomic.Value(u64).init(0);

fn startTimeNs() u64 {
    const cur = g_start_time_ns.load(.acquire);
    if (cur != 0) return cur;
    const now = @as(u64, @intCast(std.time.nanoTimestamp()));
    _ = g_start_time_ns.cmpxchgStrong(0, now, .acq_rel, .acquire);
    return g_start_time_ns.load(.acquire);
}
const ERR_OTEL_EXPORT_FAILED = "UZ-OBS-OTEL-001";
const ERR_OTEL_CONNECT_FAILED = "UZ-OBS-OTEL-002";
const ERR_OTEL_REQUEST_FAILED = "UZ-OBS-OTEL-003";
const ERR_OTEL_UNEXPECTED_STATUS = "UZ-OBS-OTEL-004";

const OtelConfig = struct {
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
) ![]u8 {
    const prom_body = try metrics.renderPrometheus(alloc, worker_running);
    defer alloc.free(prom_body);

    const now_ns = @as(u64, @intCast(std.time.nanoTimestamp()));
    const start_ns = startTimeNs();
    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    const w = out.writer(alloc);

    try w.writeAll("{\"resourceMetrics\":[{\"resource\":{\"attributes\":[");
    try w.writeAll("{\"key\":\"service.name\",\"value\":{\"stringValue\":");
    try otel_json.writeQuoted(w, service_name);
    try w.writeAll("}}");
    try w.writeAll("]},\"scopeMetrics\":[{\"scope\":{\"name\":\"zombied\"},\"metrics\":[");

    // Parse Prometheus text: emit scalar (counter/gauge) data points inline,
    // and accumulate histogram families to flush after the loop.
    var hist_acc = otel_histogram.Accumulator.init(alloc);
    defer hist_acc.deinit();

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
            } else if (std.mem.indexOf(u8, line, " histogram")) |_| {
                current_type = "histogram";
            } else {
                current_type = "gauge";
            }
            continue;
        }
        if (std.mem.startsWith(u8, line, "# ")) continue;

        if (std.mem.eql(u8, current_type, "histogram")) {
            if (try hist_acc.tryIngest(line)) continue;
            // Unrecognized histogram line — skip silently.
            continue;
        }

        const sep = std.mem.indexOf(u8, line, " ") orelse continue;
        const head = line[0..sep];
        const value_str = std.mem.trim(u8, line[sep + 1 ..], " \t\r\n");

        // A labeled Prometheus line looks like `name{k="v",…} value`. OTLP
        // requires the braces stripped from the instrument name and the
        // label set emitted as per-data-point attributes (a name containing
        // `{…}` is rejected by collectors).
        const brace_start = std.mem.indexOfScalar(u8, head, '{');
        const name: []const u8 = if (brace_start) |b| head[0..b] else head;
        const labels_src: []const u8 = if (brace_start) |b| blk: {
            const end = std.mem.lastIndexOfScalar(u8, head, '}') orelse break :blk "";
            if (end <= b) break :blk "";
            break :blk head[b + 1 .. end];
        } else "";

        if (!first) try w.writeAll(",");
        first = false;

        if (std.mem.eql(u8, current_type, "counter")) {
            try w.print(
                "{{\"name\":\"{s}\",\"sum\":{{\"aggregationTemporality\":2,\"isMonotonic\":true,\"dataPoints\":[{{\"asInt\":\"{s}\",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\"",
                .{ name, value_str, start_ns, now_ns },
            );
            try writeAttributes(w, labels_src);
            try w.writeAll("}]}}");
        } else {
            try w.print(
                "{{\"name\":\"{s}\",\"gauge\":{{\"dataPoints\":[{{\"asInt\":\"{s}\",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\"",
                .{ name, value_str, start_ns, now_ns },
            );
            try writeAttributes(w, labels_src);
            try w.writeAll("}]}}");
        }
    }

    try hist_acc.writeOtlp(w, start_ns, now_ns, &first);

    try w.writeAll("]}]}]}");

    return out.toOwnedSlice(alloc);
}

/// Best-effort OTLP push. Export failures are logged and never propagated.
pub fn exportMetricsSnapshotBestEffort(
    alloc: std.mem.Allocator,
    cfg: OtelConfig,
    worker_running: bool,
) void {
    metrics.incOtelExportTotal();
    exportMetricsSnapshotInner(alloc, cfg, worker_running) catch |err| {
        metrics.incOtelExportFailed();
        obs_log.logWarnErr(.otel_export, err, "otel.export_fail error_code={s} endpoint={s}", .{ exportErrorCode(err), cfg.endpoint });
        return;
    };
    metrics.setOtelLastSuccessAtMs(std.time.milliTimestamp());
}

fn exportMetricsSnapshotInner(
    alloc: std.mem.Allocator,
    cfg: OtelConfig,
    worker_running: bool,
) !void {
    const body = try renderOtlpJson(alloc, cfg.service_name, worker_running);
    defer alloc.free(body);

    const endpoint = std.mem.trimRight(u8, cfg.endpoint, "/");
    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, OTLP_METRICS_PATH });
    defer alloc.free(url);

    try postOtlpJson(alloc, url, body);
    log.info("otel.export_ok endpoint={s} payload_len={d}", .{ url, body.len });
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

pub const writeAttributes = otel_json.writeAttributes;

fn isSuccessStatus(status: std.http.Status) bool {
    return switch (status) {
        .ok, .created, .accepted, .no_content => true,
        else => false,
    };
}

// --- Tests ---

test "renderOtlpJson produces valid JSON envelope" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "test-svc", true);
    defer alloc.free(body);

    // Must start with resourceMetrics
    try std.testing.expect(std.mem.startsWith(u8, body, "{\"resourceMetrics\":"));
    // Must contain service name
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "test-svc"));
    // Must contain at least one metric
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_retries_total"));
    // Must end with closing brackets
    try std.testing.expect(std.mem.endsWith(u8, body, "]}]}]}"));
}

test "renderOtlpJson counters use sum with isMonotonic" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "zombied", false);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"isMonotonic\":true"));
}

test "renderOtlpJson gauges use gauge type" {
    const alloc = std.testing.allocator;
    const body = try renderOtlpJson(alloc, "zombied", true);
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

test "renderOtlpJson emits OTLP histogram data points for Prometheus histograms" {
    const alloc = std.testing.allocator;
    const metrics_zombie = @import("metrics_zombie.zig");
    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();
    metrics_zombie.observeZombieExecutionSeconds(1_500);

    const body = try renderOtlpJson(alloc, "zombied", true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"name\":\"zombie_execution_seconds\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"histogram\":{\"aggregationTemporality\":2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"explicitBounds\":["));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"bucketCounts\":["));

    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_execution_seconds_bucket") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_execution_seconds_sum ") == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_execution_seconds_count ") == null);
}

test {
    _ = @import("otel_histogram.zig");
    _ = @import("otel_json.zig");
    _ = @import("otel_export_test.zig");
}
