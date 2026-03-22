//! OTLP JSON span exporter for Grafana Cloud Tempo.
//! Ring buffer + background flush thread. Callers push completed spans;
//! this module batches and POSTs to GRAFANA_OTLP_ENDPOINT/v1/traces.
//!
//! Fire-and-forget: export errors increment counters, never block callers.
//! Config: reuses GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY.

const std = @import("std");
const metrics = @import("metrics.zig");
const obs_log = @import("logging.zig");
const otel_logs = @import("otel_logs.zig");
const trace = @import("trace.zig");

const log = std.log.scoped(.otel_traces);

const OTLP_TRACES_PATH = "/v1/traces";
const BUFFER_CAPACITY: usize = 1024;
const FLUSH_INTERVAL_MS: u64 = 5_000;
const FLUSH_BATCH_SIZE: usize = 50;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;

pub const ERR_OTEL_TRACE_EXPORT_FAILED = "UZ-OBS-OTEL-TRACE-001";
pub const ERR_OTEL_TRACE_CONNECT_FAILED = "UZ-OBS-OTEL-TRACE-002";

// ---------------------------------------------------------------------------
// Span entry
// ---------------------------------------------------------------------------

const MAX_NAME_LEN: usize = 128;
const MAX_ATTR_COUNT: usize = 8;
const MAX_ATTR_KEY_LEN: usize = 32;
const MAX_ATTR_VAL_LEN: usize = 64;

pub const SpanAttr = struct {
    key: [MAX_ATTR_KEY_LEN]u8,
    key_len: u8,
    val: [MAX_ATTR_VAL_LEN]u8,
    val_len: u8,
};

pub const SpanEntry = struct {
    trace_id: [trace.TRACE_ID_HEX_LEN]u8,
    span_id: [trace.SPAN_ID_HEX_LEN]u8,
    parent_span_id: [trace.SPAN_ID_HEX_LEN]u8,
    has_parent: bool,
    start_ns: u64,
    end_ns: u64,
    name: [MAX_NAME_LEN]u8,
    name_len: u8,
    attrs: [MAX_ATTR_COUNT]SpanAttr,
    attr_count: u8,
};

// ---------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------

const Ring = struct {
    buffer: [BUFFER_CAPACITY]SpanEntry = undefined,
    head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    fn push(self: *Ring, entry: SpanEntry) bool {
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

    fn pop(self: *Ring) ?SpanEntry {
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
var g_config: ?otel_logs.GrafanaOtlpConfig = null;
var g_flush_thread: ?std.Thread = null;
var g_running = std.atomic.Value(bool).init(false);

/// Install the async trace exporter. Starts a background flush thread.
pub fn install(cfg: otel_logs.GrafanaOtlpConfig) void {
    g_config = cfg;
    g_running.store(true, .release);
    g_flush_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
        g_config = null;
        g_running.store(false, .release);
        return;
    };
}

/// Stop the background flush thread and drain remaining spans.
pub fn uninstall() void {
    g_running.store(false, .release);
    if (g_flush_thread) |t| {
        t.join();
        g_flush_thread = null;
    }
    g_config = null;
}

/// Returns true if the async trace exporter is running.
pub fn isInstalled() bool {
    return g_running.load(.acquire) and g_config != null;
}

/// Enqueue a completed span for async export. Non-blocking, fire-and-forget.
pub fn enqueueSpan(entry: SpanEntry) void {
    if (!isInstalled()) return;
    _ = g_ring.push(entry);
}

/// Helper: build a SpanEntry from a TraceContext, name, timing, and attributes.
pub fn buildSpan(
    ctx: trace.TraceContext,
    name: []const u8,
    start_ns: u64,
    end_ns: u64,
) SpanEntry {
    var entry: SpanEntry = undefined;
    entry.trace_id = ctx.trace_id;
    entry.span_id = ctx.span_id;
    entry.has_parent = ctx.parent_span_id != null;
    if (ctx.parent_span_id) |pid| {
        entry.parent_span_id = pid;
    } else {
        entry.parent_span_id = [_]u8{0} ** trace.SPAN_ID_HEX_LEN;
    }
    entry.start_ns = start_ns;
    entry.end_ns = end_ns;
    entry.name_len = @intCast(@min(name.len, MAX_NAME_LEN));
    @memcpy(entry.name[0..entry.name_len], name[0..entry.name_len]);
    entry.attr_count = 0;
    return entry;
}

/// Add a string attribute to a span entry. Returns false if attrs are full.
pub fn addAttr(entry: *SpanEntry, key: []const u8, val: []const u8) bool {
    if (entry.attr_count >= MAX_ATTR_COUNT) return false;
    const idx = entry.attr_count;
    entry.attrs[idx].key_len = @intCast(@min(key.len, MAX_ATTR_KEY_LEN));
    @memcpy(entry.attrs[idx].key[0..entry.attrs[idx].key_len], key[0..entry.attrs[idx].key_len]);
    entry.attrs[idx].val_len = @intCast(@min(val.len, MAX_ATTR_VAL_LEN));
    @memcpy(entry.attrs[idx].val[0..entry.attrs[idx].val_len], val[0..entry.attrs[idx].val_len]);
    entry.attr_count += 1;
    return true;
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

    var payload_buf: [256 * 1024]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
    const alloc = fba.allocator();

    var spans_json: std.ArrayList(u8) = .{};
    const w = spans_json.writer(alloc);
    var first = true;

    while (count < FLUSH_BATCH_SIZE) : (count += 1) {
        const entry = g_ring.pop() orelse break;
        if (!first) w.writeAll(",") catch break;
        first = false;

        // Span JSON
        w.print(
            "{{\"traceId\":\"{s}\",\"spanId\":\"{s}\"",
            .{ entry.trace_id, entry.span_id },
        ) catch break;

        if (entry.has_parent) {
            w.print(",\"parentSpanId\":\"{s}\"", .{entry.parent_span_id}) catch break;
        }

        w.print(
            ",\"name\":\"{f}\",\"kind\":1,\"startTimeUnixNano\":\"{d}\",\"endTimeUnixNano\":\"{d}\"",
            .{
                std.json.fmt(entry.name[0..entry.name_len], .{}),
                entry.start_ns,
                entry.end_ns,
            },
        ) catch break;

        // Attributes
        if (entry.attr_count > 0) {
            w.writeAll(",\"attributes\":[") catch break;
            var ai: u8 = 0;
            while (ai < entry.attr_count) : (ai += 1) {
                if (ai > 0) w.writeAll(",") catch break;
                w.print(
                    "{{\"key\":\"{f}\",\"value\":{{\"stringValue\":\"{f}\"}}}}",
                    .{
                        std.json.fmt(entry.attrs[ai].key[0..entry.attrs[ai].key_len], .{}),
                        std.json.fmt(entry.attrs[ai].val[0..entry.attrs[ai].val_len], .{}),
                    },
                ) catch break;
            }
            w.writeAll("]") catch break;
        }

        w.writeAll("}") catch break;
    }

    if (count == 0) return;

    const spans_data = spans_json.toOwnedSlice(alloc) catch return;

    var full_payload: std.ArrayList(u8) = .{};
    const fw = full_payload.writer(alloc);
    fw.print(
        "{{\"resourceSpans\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}},\"scopeSpans\":[{{\"scope\":{{\"name\":\"zombied\"}},\"spans\":[{s}]}}]}}]}}",
        .{ cfg.service_name, spans_data },
    ) catch return;

    const body = full_payload.toOwnedSlice(alloc) catch return;

    otel_logs.postWithBasicAuth(alloc, cfg, OTLP_TRACES_PATH, body) catch {};
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "ring buffer push and pop round-trip" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const ctx = trace.TraceContext.generate();
    var entry = buildSpan(ctx, "test.span", 1000, 2000);
    try std.testing.expect(addAttr(&entry, "key1", "val1"));

    try std.testing.expect(ring.push(entry));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(u64, 1000), popped.?.start_ns);
    try std.testing.expectEqual(@as(u64, 2000), popped.?.end_ns);
    try std.testing.expectEqualStrings("test.span", popped.?.name[0..popped.?.name_len]);
    try std.testing.expectEqual(@as(u8, 1), popped.?.attr_count);
    try std.testing.expectEqualStrings("key1", popped.?.attrs[0].key[0..popped.?.attrs[0].key_len]);
    try std.testing.expectEqualStrings("val1", popped.?.attrs[0].val[0..popped.?.attrs[0].val_len]);
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
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const ctx = trace.TraceContext.generate();
    const entry = buildSpan(ctx, "x", 0, 0);

    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(entry));
    }
    try std.testing.expect(!ring.push(entry));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "enqueueSpan is no-op when exporter not installed" {
    const ctx = trace.TraceContext.generate();
    const entry = buildSpan(ctx, "noop", 0, 0);
    enqueueSpan(entry);
}

test "buildSpan sets fields correctly for root span" {
    const ctx = trace.TraceContext.generate();
    const entry = buildSpan(ctx, "root.op", 100, 200);
    try std.testing.expect(!entry.has_parent);
    try std.testing.expectEqualStrings("root.op", entry.name[0..entry.name_len]);
    try std.testing.expectEqual(@as(u64, 100), entry.start_ns);
    try std.testing.expectEqual(@as(u64, 200), entry.end_ns);
    try std.testing.expectEqual(@as(u8, 0), entry.attr_count);
}

test "buildSpan sets parent for child span" {
    const root = trace.TraceContext.generate();
    const child = root.child();
    const entry = buildSpan(child, "child.op", 100, 200);
    try std.testing.expect(entry.has_parent);
    try std.testing.expectEqualSlices(u8, &root.span_id, &entry.parent_span_id);
}

test "addAttr respects max count" {
    const ctx = trace.TraceContext.generate();
    var entry = buildSpan(ctx, "attrs", 0, 0);
    var i: u8 = 0;
    while (i < MAX_ATTR_COUNT) : (i += 1) {
        try std.testing.expect(addAttr(&entry, "k", "v"));
    }
    try std.testing.expect(!addAttr(&entry, "overflow", "x"));
    try std.testing.expectEqual(@as(u8, MAX_ATTR_COUNT), entry.attr_count);
}

test "SpanEntry truncates oversized name" {
    const ctx = trace.TraceContext.generate();
    const long_name = "a" ** 200;
    const entry = buildSpan(ctx, long_name, 0, 0);
    try std.testing.expectEqual(@as(u8, MAX_NAME_LEN), entry.name_len);
}
