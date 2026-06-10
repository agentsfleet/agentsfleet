//! OTLP JSON log exporter for Grafana Cloud Loki.
//! Ring buffer + background flush thread. Dual-write: callers push log
//! entries; this module batches and POSTs to GRAFANA_OTLP_ENDPOINT/v1/logs.
//!
//! Fire-and-forget: export errors increment counters, never block callers.
//! Config: GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const StringBuilder = @import("../util/strings/string_builder.zig");
const env_resolve = @import("../config/env_resolve.zig");

const EnvMap = common.env.Map;

const OTLP_LOGS_PATH = "/v1/logs";
const BUFFER_CAPACITY: usize = 2048;
const FLUSH_INTERVAL_MS: u64 = 5_000;
const FLUSH_BATCH_SIZE: usize = 50;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;

const logging = @import("log");
const log = logging.scoped(.otel_logs);
pub const GrafanaOtlpConfig = struct {
    endpoint: []const u8,
    instance_id: []const u8,
    api_key: []const u8,
    service_name: []const u8 = "zombied",
};

/// Try to load Grafana OTLP config from environment. Returns null when not configured.
pub fn configFromEnv(env_map: *const EnvMap, alloc: std.mem.Allocator) ?GrafanaOtlpConfig {
    const endpoint = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_ENDPOINT") orelse return null;
    const trimmed = std.mem.trim(u8, endpoint, " \t\r\n");
    if (trimmed.len == 0) {
        alloc.free(endpoint);
        return null;
    }
    const instance_id = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_INSTANCE_ID") orelse {
        alloc.free(endpoint);
        return null;
    };
    const api_key = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_API_KEY") orelse {
        alloc.free(endpoint);
        alloc.free(instance_id);
        return null;
    };
    const service_name = env_resolve.config(env_map, alloc, "OTEL_SERVICE_NAME") orelse {
        return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key };
    };
    return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key, .service_name = service_name };
}

// ---------------------------------------------------------------------------
// Log entry ring buffer
// ---------------------------------------------------------------------------

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

const Ring = struct {
    // SAFETY: each slot is written by exactly one claiming producer before that
    // slot's ready flag is published; pop reads only ready slots.
    buffer: [BUFFER_CAPACITY]LogEntry = undefined,
    ready: [BUFFER_CAPACITY]std.atomic.Value(u8) = [_]std.atomic.Value(u8){std.atomic.Value(u8).init(0)} ** BUFFER_CAPACITY,
    head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    pub fn push(self: *Ring, entry: LogEntry) bool {
        while (true) {
            const head = self.head.load(.acquire);
            const tail = self.tail.load(.acquire);
            const next_head = (head + 1) % BUFFER_CAPACITY;
            if (next_head == tail) {
                // safe because: independent statistic; no ordering required.
                _ = self.dropped.fetchAdd(1, .monotonic);
                return false;
            }
            // safe because: the .acq_rel cmpxchg claims slot `head` exclusively —
            // a losing producer observes the new head and retries, so two
            // producers can never write the same slot (the previous plain
            // load/store pair allowed exactly that torn write). Failure order
            // .acquire re-reads a coherent head.
            if (self.head.cmpxchgWeak(head, next_head, .acq_rel, .acquire)) |_| continue;
            self.buffer[head] = entry;
            // safe because: .release publishes the completed slot write to
            // pop()'s .acquire load of the same flag.
            self.ready[head].store(1, .release);
            return true;
        }
    }

    pub fn pop(self: *Ring) ?LogEntry {
        // safe because: tail is consumer-owned (single flush thread); head's
        // .acquire pairs with producers' claim cmpxchg.
        const tail = self.tail.load(.acquire);
        const head = self.head.load(.acquire);
        if (head == tail) return null;
        // safe because: .acquire pairs with the producer's ready .release store.
        // A claimed-but-unwritten head-of-line slot reads 0 → treat as empty for
        // this pass; the next flush pass retries (bound: the producer's memcpy).
        if (self.ready[tail].load(.acquire) != 1) return null;
        const entry = self.buffer[tail];
        // safe because: ready clears before the tail .release store, and a
        // producer can only claim this slot after observing the advanced tail —
        // so a fresh claimant always starts from ready == 0.
        self.ready[tail].store(0, .release);
        self.tail.store((tail + 1) % BUFFER_CAPACITY, .release);
        return entry;
    }

    pub fn len(self: *Ring) usize {
        // safe because: monotonic-quality snapshot for batching/drain heuristics
        // only; .acquire keeps it no staler than the callers' own loads.
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

    // SAFETY: written by surrounding init logic before any read of this storage.
    var entry: LogEntry = undefined;
    entry.timestamp_ns = @intCast(clock.nowNanos());
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
        common.sleepNanos(FLUSH_INTERVAL_MS * std.time.ns_per_ms);
        flushBatch();
    }
    // Drain on shutdown
    const deadline = clock.nowMillis() + @as(i64, @intCast(SHUTDOWN_DRAIN_TIMEOUT_MS));
    while (g_ring.len() > 0 and clock.nowMillis() < deadline) {
        flushBatch();
    }
}

fn flushBatch() void {
    const cfg = g_config orelse return;
    var count: usize = 0;

    // Build OTLP JSON payload from buffered entries
    const OTLP_PAYLOAD_BUF_BYTES = 256 * 1024;
    var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
    const alloc = fba.allocator();

    var entries: std.ArrayList(u8) = .empty;
    var first = true;

    while (count < FLUSH_BATCH_SIZE) : (count += 1) {
        const entry = g_ring.pop() orelse break;
        if (!first) entries.appendSlice(alloc, ",") catch break;
        first = false;

        entries.print(
            alloc,
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

    const envelope_fmt = "{{\"resourceLogs\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}},\"scopeLogs\":[{{\"scope\":{{\"name\":\"zombied\"}},\"logRecords\":[{s}]}}]}}]}}";
    const envelope_args = .{ cfg.service_name, log_records };

    var sb: StringBuilder = .{};
    defer sb.deinit(alloc);
    sb.fmtCount(envelope_fmt, envelope_args);
    sb.allocate(alloc) catch return;
    const body = sb.fmt(envelope_fmt, envelope_args);

    postWithBasicAuth(alloc, cfg, OTLP_LOGS_PATH, body) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

pub fn postWithBasicAuth(
    alloc: std.mem.Allocator,
    cfg: GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
) !void {
    // Background flush thread: a blocking one-shot POST (not an async event
    // loop), so the process-global blocking io is the correct source.
    var client: std.http.Client = .{ .allocator = alloc, .io = common.globalIo() };
    defer client.deinit();

    const endpoint = std.mem.trimEnd(u8, cfg.endpoint, "/");
    const url = std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, path }) catch return;

    // Basic auth: base64(instance_id:api_key)
    var auth_raw_buf: [512]u8 = undefined;
    const auth_raw = std.fmt.bufPrint(&auth_raw_buf, "{s}:{s}", .{ cfg.instance_id, cfg.api_key }) catch return;
    const AUTH_B64_BUF_BYTES = 1024;
    var b64_buf: [AUTH_B64_BUF_BYTES]u8 = undefined;
    const b64_len = std.base64.standard.Encoder.calcSize(auth_raw.len);
    if (b64_len > b64_buf.len) return;
    const b64 = b64_buf[0..b64_len];
    _ = std.base64.standard.Encoder.encode(b64, auth_raw);
    var auth_header_buf: [1100]u8 = undefined;
    const auth_header = std.fmt.bufPrint(&auth_header_buf, "Basic {s}", .{b64}) catch return;

    var resp_body: std.ArrayList(u8) = .empty;
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

    // Non-2xx is a rejected export, not a success — surface it so the callers'
    // existing catch-warn logs it (the prior bare `return` here was a no-op
    // tail branch that swallowed rejections silently).
    if (result.status != .ok and result.status != .no_content and result.status != .accepted) {
        return error.OtlpExportRejected;
    }
}

pub const TestRing = Ring;
pub const TestLogEntry = LogEntry;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

test {
    _ = @import("otel_logs_test.zig");
}
