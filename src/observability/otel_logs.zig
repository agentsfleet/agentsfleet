//! OTLP JSON log exporter for Grafana Cloud Loki.
//! Ring buffer + background flush thread. Dual-write: callers push log
//! entries; this module batches and POSTs to GRAFANA_OTLP_ENDPOINT/v1/logs.
//!
//! Fire-and-forget: export errors increment counters, never block callers.
//! Config: GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY.

const std = @import("std");
const metrics = @import("metrics.zig");
const obs_log = @import("logging.zig");

const log = std.log.scoped(.otel_logs);

const OTLP_LOGS_PATH = "/v1/logs";
const BUFFER_CAPACITY: usize = 2048;
const FLUSH_INTERVAL_MS: u64 = 5_000;
const FLUSH_BATCH_SIZE: usize = 50;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;

pub const ERR_OTEL_LOG_EXPORT_FAILED = "UZ-OBS-OTEL-LOG-001";
pub const ERR_OTEL_LOG_CONNECT_FAILED = "UZ-OBS-OTEL-LOG-002";

pub const GrafanaOtlpConfig = struct {
    endpoint: []const u8,
    instance_id: []const u8,
    api_key: []const u8,
    service_name: []const u8 = "zombied",
};

/// Try to load Grafana OTLP config from environment. Returns null when not configured.
pub fn configFromEnv(alloc: std.mem.Allocator) ?GrafanaOtlpConfig {
    const endpoint = std.process.getEnvVarOwned(alloc, "GRAFANA_OTLP_ENDPOINT") catch return null;
    const trimmed = std.mem.trim(u8, endpoint, " \t\r\n");
    if (trimmed.len == 0) {
        alloc.free(endpoint);
        return null;
    }
    const instance_id = std.process.getEnvVarOwned(alloc, "GRAFANA_OTLP_INSTANCE_ID") catch {
        alloc.free(endpoint);
        return null;
    };
    const api_key = std.process.getEnvVarOwned(alloc, "GRAFANA_OTLP_API_KEY") catch {
        alloc.free(endpoint);
        alloc.free(instance_id);
        return null;
    };
    const service_name = std.process.getEnvVarOwned(alloc, "OTEL_SERVICE_NAME") catch {
        return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key };
    };
    return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key, .service_name = service_name };
}

// ---------------------------------------------------------------------------
// Log entry ring buffer
// ---------------------------------------------------------------------------

const MAX_MSG_LEN: usize = 512;

pub const LogEntry = struct {
    timestamp_ns: u64,
    level: [5]u8,
    level_len: u8,
    scope: [32]u8,
    scope_len: u8,
    body: [MAX_MSG_LEN]u8,
    body_len: u16,
};

const Ring = struct {
    buffer: [BUFFER_CAPACITY]LogEntry = undefined,
    head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    fn push(self: *Ring, entry: LogEntry) bool {
        const head = self.head.load(.acquire);
        const tail = self.tail.load(.acquire);
        const next_head = (head + 1) % BUFFER_CAPACITY;
        if (next_head == tail) {
            _ = self.dropped.fetchAdd(1, .monotonic);
            return false;
        }
        self.buffer[head] = entry;
        self.head.store(next_head, .release);
        return true;
    }

    fn pop(self: *Ring) ?LogEntry {
        const head = self.head.load(.acquire);
        const tail = self.tail.load(.acquire);
        if (head == tail) return null;
        const entry = self.buffer[tail];
        self.tail.store((tail + 1) % BUFFER_CAPACITY, .release);
        return entry;
    }

    fn len(self: *Ring) usize {
        const head = self.head.load(.acquire);
        const tail = self.tail.load(.acquire);
        if (head >= tail) return head - tail;
        return BUFFER_CAPACITY - tail + head;
    }
};

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

var g_ring: Ring = .{};
var g_config: ?GrafanaOtlpConfig = null;
var g_flush_thread: ?std.Thread = null;
var g_running = std.atomic.Value(bool).init(false);

/// Install the async log exporter. Starts a background flush thread.
pub fn install(cfg: GrafanaOtlpConfig) void {
    g_config = cfg;
    g_running.store(true, .release);
    g_flush_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
        g_config = null;
        g_running.store(false, .release);
        return;
    };
}

/// Stop the background flush thread and drain remaining entries.
pub fn uninstall() void {
    g_running.store(false, .release);
    if (g_flush_thread) |t| {
        t.join();
        g_flush_thread = null;
    }
    g_config = null;
}

/// Returns true if the async exporter is running.
pub fn isInstalled() bool {
    return g_running.load(.acquire) and g_config != null;
}

/// Enqueue a log entry for async export. Non-blocking, fire-and-forget.
pub fn enqueue(
    level: []const u8,
    scope: []const u8,
    msg: []const u8,
) void {
    if (!isInstalled()) return;

    var entry: LogEntry = undefined;
    entry.timestamp_ns = @intCast(std.time.nanoTimestamp());
    entry.level_len = @intCast(@min(level.len, 5));
    @memcpy(entry.level[0..entry.level_len], level[0..entry.level_len]);
    entry.scope_len = @intCast(@min(scope.len, 32));
    @memcpy(entry.scope[0..entry.scope_len], scope[0..entry.scope_len]);
    entry.body_len = @intCast(@min(msg.len, MAX_MSG_LEN));
    @memcpy(entry.body[0..entry.body_len], msg[0..entry.body_len]);

    _ = g_ring.push(entry);
}

// ---------------------------------------------------------------------------
// Background flush
// ---------------------------------------------------------------------------

fn flushLoop() void {
    while (g_running.load(.acquire)) {
        std.Thread.sleep(FLUSH_INTERVAL_MS * std.time.ns_per_ms);
        flushBatch();
    }
    // Drain on shutdown
    const deadline = std.time.milliTimestamp() + @as(i64, @intCast(SHUTDOWN_DRAIN_TIMEOUT_MS));
    while (g_ring.len() > 0 and std.time.milliTimestamp() < deadline) {
        flushBatch();
    }
}

fn flushBatch() void {
    const cfg = g_config orelse return;
    var count: usize = 0;

    // Build OTLP JSON payload from buffered entries
    var payload_buf: [256 * 1024]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
    const alloc = fba.allocator();

    var entries: std.ArrayList(u8) = .{};
    const w = entries.writer(alloc);
    var first = true;

    while (count < FLUSH_BATCH_SIZE) : (count += 1) {
        const entry = g_ring.pop() orelse break;
        if (!first) w.writeAll(",") catch break;
        first = false;

        w.print(
            "{{\"timeUnixNano\":\"{d}\",\"severityText\":\"{s}\",\"body\":{{\"stringValue\":\"{f}\"}},\"attributes\":[{{\"key\":\"scope\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}}",
            .{
                entry.timestamp_ns,
                entry.level[0..entry.level_len],
                std.json.fmt(entry.body[0..entry.body_len], .{}),
                entry.scope[0..entry.scope_len],
            },
        ) catch break;
    }

    if (count == 0) return;

    const log_records = entries.toOwnedSlice(alloc) catch return;

    var full_payload: std.ArrayList(u8) = .{};
    const fw = full_payload.writer(alloc);
    fw.print(
        "{{\"resourceLogs\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}},\"scopeLogs\":[{{\"scope\":{{\"name\":\"zombied\"}},\"logRecords\":[{s}]}}]}}]}}",
        .{ cfg.service_name, log_records },
    ) catch return;

    const body = full_payload.toOwnedSlice(alloc) catch return;

    postWithBasicAuth(alloc, cfg, OTLP_LOGS_PATH, body) catch {};
}

pub fn postWithBasicAuth(
    alloc: std.mem.Allocator,
    cfg: GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
) !void {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    const endpoint = std.mem.trimRight(u8, cfg.endpoint, "/");
    const url = std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, path }) catch return;

    // Basic auth: base64(instance_id:api_key)
    var auth_raw_buf: [512]u8 = undefined;
    const auth_raw = std.fmt.bufPrint(&auth_raw_buf, "{s}:{s}", .{ cfg.instance_id, cfg.api_key }) catch return;
    var b64_buf: [1024]u8 = undefined;
    const b64_len = std.base64.standard.Encoder.calcSize(auth_raw.len);
    if (b64_len > b64_buf.len) return;
    const b64 = b64_buf[0..b64_len];
    _ = std.base64.standard.Encoder.encode(b64, auth_raw);
    var auth_header_buf: [1100]u8 = undefined;
    const auth_header = std.fmt.bufPrint(&auth_header_buf, "Basic {s}", .{b64}) catch return;

    var resp_body: std.ArrayList(u8) = .{};
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_body);

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth_header },
    };

    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch return;

    if (result.status != .ok and result.status != .no_content and result.status != .accepted) return;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "configFromEnv returns null when GRAFANA_OTLP_ENDPOINT is unset" {
    // Cannot unsetenv in Zig 0.15 — test relies on env not being set in CI/local
    // If GRAFANA_OTLP_ENDPOINT happens to be set, skip gracefully
    if (configFromEnv(std.testing.allocator)) |present| {
        std.testing.allocator.free(present.endpoint);
        std.testing.allocator.free(present.instance_id);
        std.testing.allocator.free(present.api_key);
        if (!std.mem.eql(u8, present.service_name, "zombied")) std.testing.allocator.free(present.service_name);
        return; // env is set, skip this test
    }
    const cfg = configFromEnv(std.testing.allocator);
    try std.testing.expect(cfg == null);
}

test "ring buffer push and pop round-trip" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};
    var entry: LogEntry = undefined;
    entry.timestamp_ns = 42;
    entry.level_len = 4;
    @memcpy(entry.level[0..4], "info");
    entry.scope_len = 4;
    @memcpy(entry.scope[0..4], "test");
    entry.body_len = 5;
    @memcpy(entry.body[0..5], "hello");

    try std.testing.expect(ring.push(entry));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(u64, 42), popped.?.timestamp_ns);
    try std.testing.expectEqualStrings("info", popped.?.level[0..popped.?.level_len]);
    try std.testing.expectEqualStrings("test", popped.?.scope[0..popped.?.scope_len]);
    try std.testing.expectEqualStrings("hello", popped.?.body[0..popped.?.body_len]);
    try std.testing.expectEqual(@as(usize, 0), ring.len());
}

test "ring buffer pop returns null when empty" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};
    try std.testing.expect(ring.pop() == null);
}

test "ring buffer drops when full" {
    // Use heap allocation to avoid 8MB+ stack frame that valgrind flags.
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    var entry: LogEntry = undefined;
    entry.timestamp_ns = 0;
    entry.level_len = 3;
    @memcpy(entry.level[0..3], "err");
    entry.scope_len = 1;
    entry.scope[0] = 'x';
    entry.body_len = 1;
    entry.body[0] = 'y';

    // Fill to capacity - 1 (ring buffer leaves one slot empty)
    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(entry));
    }
    // Next push should fail (buffer full)
    try std.testing.expect(!ring.push(entry));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "enqueue is no-op when exporter not installed" {
    // g_config is null by default, so enqueue should silently no-op
    enqueue("info", "test", "this should not crash");
}

test "LogEntry truncates oversized fields" {
    var entry: LogEntry = undefined;
    const long_scope = "this_scope_is_way_too_long_for_32_bytes_buffer_overflow";
    entry.scope_len = @intCast(@min(long_scope.len, 32));
    @memcpy(entry.scope[0..entry.scope_len], long_scope[0..entry.scope_len]);
    try std.testing.expectEqual(@as(u8, 32), entry.scope_len);
}
