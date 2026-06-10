//! OTLP JSON span exporter for Grafana Cloud Tempo.
//! Ring buffer + background flush thread. Callers push completed spans;
//! this module batches and POSTs to GRAFANA_OTLP_ENDPOINT/v1/traces.
//!
//! Fire-and-forget: export errors increment counters, never block callers.
//! Config: reuses GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const otel_logs = @import("otel_logs.zig");
const trace = @import("trace.zig");
const StringBuilder = @import("../util/strings/string_builder.zig");

const OTLP_TRACES_PATH = "/v1/traces";
const BUFFER_CAPACITY: usize = 1024;
const FLUSH_INTERVAL_MS: u64 = 5_000;
const FLUSH_BATCH_SIZE: usize = 50;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;

const logging = @import("log");
const log = logging.scoped(.otel_traces);
// ---------------------------------------------------------------------------
// Span entry
// ---------------------------------------------------------------------------

const MAX_NAME_LEN: usize = 128;
const MAX_ATTR_COUNT: usize = 12;
const MAX_ATTR_KEY_LEN: usize = 32;
const MAX_ATTR_VAL_LEN: usize = 64;

const SpanAttr = struct {
    key: [MAX_ATTR_KEY_LEN]u8,
    key_len: u8,
    val: [MAX_ATTR_VAL_LEN]u8,
    val_len: u8,
};

const SpanEntry = struct {
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
    // SAFETY: each slot is written by exactly one claiming producer before that
    // slot's ready flag is published; pop reads only ready slots.
    buffer: [BUFFER_CAPACITY]SpanEntry = undefined,
    ready: [BUFFER_CAPACITY]std.atomic.Value(u8) = [_]std.atomic.Value(u8){std.atomic.Value(u8).init(0)} ** BUFFER_CAPACITY,
    head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    pub fn push(self: *Ring, entry: SpanEntry) bool {
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

    pub fn pop(self: *Ring) ?SpanEntry {
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
    // SAFETY: written by surrounding init logic before any read of this storage.
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

    const OTLP_PAYLOAD_BUF_BYTES = 256 * 1024;
    var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
    const alloc = fba.allocator();

    var spans_json: std.ArrayList(u8) = .empty;
    var first = true;

    while (count < FLUSH_BATCH_SIZE) : (count += 1) {
        const entry = g_ring.pop() orelse break;
        if (!first) spans_json.appendSlice(alloc, ",") catch break;
        first = false;

        // Span JSON
        spans_json.print(
            alloc,
            "{{\"traceId\":\"{s}\",\"spanId\":\"{s}\"",
            .{ entry.trace_id, entry.span_id },
        ) catch break;

        if (entry.has_parent) {
            spans_json.print(alloc, ",\"parentSpanId\":\"{s}\"", .{entry.parent_span_id}) catch break;
        }

        spans_json.print(
            alloc,
            ",\"name\":\"{f}\",\"kind\":1,\"startTimeUnixNano\":\"{d}\",\"endTimeUnixNano\":\"{d}\"",
            .{
                std.json.fmt(entry.name[0..entry.name_len], .{}),
                entry.start_ns,
                entry.end_ns,
            },
        ) catch break;

        // Attributes
        if (entry.attr_count > 0) {
            spans_json.appendSlice(alloc, ",\"attributes\":[") catch break;
            var ai: u8 = 0;
            while (ai < entry.attr_count) : (ai += 1) {
                if (ai > 0) spans_json.appendSlice(alloc, ",") catch break;
                spans_json.print(
                    alloc,
                    "{{\"key\":\"{f}\",\"value\":{{\"stringValue\":\"{f}\"}}}}",
                    .{
                        std.json.fmt(entry.attrs[ai].key[0..entry.attrs[ai].key_len], .{}),
                        std.json.fmt(entry.attrs[ai].val[0..entry.attrs[ai].val_len], .{}),
                    },
                ) catch break;
            }
            spans_json.appendSlice(alloc, "]") catch break;
        }

        spans_json.appendSlice(alloc, "}") catch break;
    }

    if (count == 0) return;

    const spans_data = spans_json.toOwnedSlice(alloc) catch return;

    const envelope_fmt = "{{\"resourceSpans\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}},\"scopeSpans\":[{{\"scope\":{{\"name\":\"zombied\"}},\"spans\":[{s}]}}]}}]}}";
    const envelope_args = .{ cfg.service_name, spans_data };

    var sb: StringBuilder = .{};
    defer sb.deinit(alloc);
    sb.fmtCount(envelope_fmt, envelope_args);
    sb.allocate(alloc) catch return;
    const body = sb.fmt(envelope_fmt, envelope_args);

    otel_logs.postWithBasicAuth(alloc, cfg, OTLP_TRACES_PATH, body) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

pub const TestRing = Ring;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;
pub const TEST_MAX_ATTR_COUNT = MAX_ATTR_COUNT;
pub const TEST_MAX_NAME_LEN = MAX_NAME_LEN;

test {
    _ = @import("otel_traces_test.zig");
}
