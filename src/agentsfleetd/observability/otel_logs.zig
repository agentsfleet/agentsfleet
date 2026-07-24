//! OpenTelemetry Protocol (OTLP) JSON log exporter for Grafana Cloud Loki.
//! Callers push log entries; the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/logs on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate. Config + the basic-auth POST that
//! used to live here moved to otlp/config.zig + otlp/Client.zig (shared by all
//! three signals); this file now carries only the log-entry shape, its
//! serialization, and the enqueue API.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const health = @import("metrics_otel.zig");
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");

const OTLP_LOGS_PATH = "/v1/logs";
const BUFFER_CAPACITY: usize = 2048;
const FLUSH_BATCH_SIZE: usize = 50;
const MAX_MSG_LEN: usize = 512;

const LogEntry = struct {
    timestamp_ns: u64,
    level: [5]u8,
    level_len: u8,
    scope: [32]u8,
    scope_len: u8,
    body: [MAX_MSG_LEN]u8,
    body_len: u16,
};

const RingT = otlp_ring.Ring(LogEntry, BUFFER_CAPACITY);
var g_ring: RingT = .{};

const Exporter = otlp_exporter.Exporter(.{
    .signal = .logs,
    .path = OTLP_LOGS_PATH,
    .scope = .otel_logs,
    .collect = collectLogs,
    .pending_count = logsPendingCount,
    .wake_threshold = FLUSH_BATCH_SIZE,
});

pub const install = Exporter.install;
pub const uninstall = Exporter.uninstall;
pub const isInstalled = Exporter.isInstalled;

/// Enqueue a log entry for async export. Non-blocking, fire-and-forget.
pub fn enqueue(
    level: []const u8,
    scope: []const u8,
    msg: []const u8,
) void {
    if (!Exporter.isInstalled()) return;

    // SAFETY: written by surrounding init logic before any read of this storage.
    var entry: LogEntry = undefined;
    entry.timestamp_ns = @intCast(clock.nowNanos());
    entry.level_len = @intCast(@min(level.len, 5));
    @memcpy(entry.level[0..entry.level_len], level[0..entry.level_len]);
    entry.scope_len = @intCast(@min(scope.len, 32));
    @memcpy(entry.scope[0..entry.scope_len], scope[0..entry.scope_len]);
    entry.body_len = @intCast(@min(msg.len, MAX_MSG_LEN));
    @memcpy(entry.body[0..entry.body_len], msg[0..entry.body_len]);

    if (g_ring.push(entry)) {
        health.setQueueDepth(.logs, g_ring.len());
        Exporter.notifyAccepted();
    } else {
        health.recordDiscard(.logs, .ring_full, 1);
    }
}

// ---------------------------------------------------------------------------
// Serialization (the exporter's collect hook)
// ---------------------------------------------------------------------------

fn logsPendingCount() usize {
    return g_ring.len();
}

fn collectLogs(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    max_entries: usize,
) otlp_exporter.CollectResult {
    if (max_entries == 0) return .empty;
    var removed: usize = 0;
    const body = collectLogsBody(alloc, cfg, max_entries, &removed) catch {
        return .{ .serialize_failed = removed };
    };
    if (removed == 0) return .empty;
    return .{ .ready = .{
        .body = body,
        .removed_count = removed,
        .export_count = removed,
    } };
}

fn collectLogsBody(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    max_entries: usize,
    removed: *usize,
) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.print(
        alloc,
        "{{\"resourceLogs\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":{f}}}}}]}},\"scopeLogs\":[{{\"scope\":{{\"name\":\"agentsfleetd\"}},\"logRecords\":[",
        .{std.json.fmt(cfg.service_name, .{})},
    );

    var first = true;
    const limit = @min(max_entries, FLUSH_BATCH_SIZE);
    while (removed.* < limit) {
        const entry = g_ring.pop() orelse break;
        removed.* += 1;
        if (!first) try out.appendSlice(alloc, ",");
        first = false;

        // json.fmt emits the surrounding quotes for the body; the format must
        // NOT wrap {f} in its own quotes (that produced invalid ""text"").
        try out.print(
            alloc,
            "{{\"timeUnixNano\":\"{d}\",\"severityText\":\"{s}\",\"body\":{{\"stringValue\":{f}}},\"attributes\":[{{\"key\":\"scope\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}}",
            .{
                entry.timestamp_ns,
                entry.level[0..entry.level_len],
                std.json.fmt(entry.body[0..entry.body_len], .{}),
                entry.scope[0..entry.scope_len],
            },
        );
    }

    // Ring drained empty. The envelope prefix is already in `out`, and errdefer
    // does not cover a successful return — free it here or every empty collect
    // strands one buffer.
    if (removed.* == 0) {
        out.deinit(alloc);
        return &.{};
    }

    try out.appendSlice(alloc, "]}]}]}");
    return try out.toOwnedSlice(alloc);
}

pub const TestRing = RingT;
pub const TestLogEntry = LogEntry;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

/// Test hook: mark installed without spawning the flush thread.
pub fn testSetInstalled(cfg: otlp_config.GrafanaOtlpConfig) void {
    Exporter.testSetInstalled(common.globalIo(), cfg);
}

/// Test hook: clear installed state and drain the ring.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
    health.setQueueDepth(.logs, 0);
}

/// Test hook: run one collect (drain + serialize the buffered log records).
pub fn testCollect(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    return switch (collectLogs(alloc, cfg, FLUSH_BATCH_SIZE)) {
        .empty => null,
        .ready => |batch| batch.body,
        .serialize_failed => error.SerializationFailed,
    };
}

/// Test hook: number of entries pending in the production ring.
pub fn testPendingCount() usize {
    return logsPendingCount();
}

/// Test hook: accepted pushes counted toward the next exporter cycle.
pub fn testAcceptedSinceCycle() u32 {
    return Exporter.testAcceptedSinceCycle();
}

test {
    _ = @import("otel_logs_test.zig");
}
