//! OTLP JSON span exporter for Grafana Cloud Tempo.
//! Callers push completed spans; the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/traces on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate (Ring + Exporter + config + post):
//! this file now carries only the span shape, the span serialization, and the
//! enqueue API. Lifecycle/thread/POST are the shared substrate.

const std = @import("std");
const trace = @import("trace.zig");
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");

const OTLP_TRACES_PATH = "/v1/traces";
const BUFFER_CAPACITY: usize = 1024;
const FLUSH_BATCH_SIZE: usize = 50;

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
// Buffer + exporter (shared substrate)
// ---------------------------------------------------------------------------

const RingT = otlp_ring.Ring(SpanEntry, BUFFER_CAPACITY);
var g_ring: RingT = .{};

const Exporter = otlp_exporter.Exporter(.{
    .path = OTLP_TRACES_PATH,
    .scope = .otel_traces,
    .collect = collectSpans,
    .pending = spansPending,
    .wake_threshold = FLUSH_BATCH_SIZE,
});

pub const install = Exporter.install;
pub const uninstall = Exporter.uninstall;
pub const isInstalled = Exporter.isInstalled;

/// Enqueue a completed span for async export. Non-blocking, fire-and-forget.
pub fn enqueueSpan(entry: SpanEntry) void {
    if (!Exporter.isInstalled()) return;
    _ = g_ring.push(entry);
    Exporter.notify();
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
// Serialization (the exporter's collect hook)
// ---------------------------------------------------------------------------

fn spansPending() bool {
    return g_ring.len() > 0;
}

fn collectSpans(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    var out: std.ArrayList(u8) = .empty;
    try out.print(
        alloc,
        "{{\"resourceSpans\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":{f}}}}}]}},\"scopeSpans\":[{{\"scope\":{{\"name\":\"agentsfleetd\"}},\"spans\":[",
        .{std.json.fmt(cfg.service_name, .{})},
    );

    var count: usize = 0;
    var first = true;
    while (count < FLUSH_BATCH_SIZE) : (count += 1) {
        const entry = g_ring.pop() orelse break;
        if (!first) try out.appendSlice(alloc, ",");
        first = false;

        try out.print(alloc, "{{\"traceId\":\"{s}\",\"spanId\":\"{s}\"", .{ entry.trace_id, entry.span_id });
        if (entry.has_parent) {
            try out.print(alloc, ",\"parentSpanId\":\"{s}\"", .{entry.parent_span_id});
        }
        // json.fmt emits the surrounding quotes (and escapes); the format must
        // NOT wrap {f} in its own quotes — doing so produced invalid ""value"".
        try out.print(
            alloc,
            ",\"name\":{f},\"kind\":1,\"startTimeUnixNano\":\"{d}\",\"endTimeUnixNano\":\"{d}\"",
            .{ std.json.fmt(entry.name[0..entry.name_len], .{}), entry.start_ns, entry.end_ns },
        );

        if (entry.attr_count > 0) {
            try out.appendSlice(alloc, ",\"attributes\":[");
            var ai: u8 = 0;
            while (ai < entry.attr_count) : (ai += 1) {
                if (ai > 0) try out.appendSlice(alloc, ",");
                try out.print(
                    alloc,
                    "{{\"key\":{f},\"value\":{{\"stringValue\":{f}}}}}",
                    .{
                        std.json.fmt(entry.attrs[ai].key[0..entry.attrs[ai].key_len], .{}),
                        std.json.fmt(entry.attrs[ai].val[0..entry.attrs[ai].val_len], .{}),
                    },
                );
            }
            try out.appendSlice(alloc, "]");
        }

        try out.appendSlice(alloc, "}");
    }

    if (count == 0) return null;

    try out.appendSlice(alloc, "]}]}]}");
    return try out.toOwnedSlice(alloc);
}

pub const TestRing = RingT;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;
pub const TEST_MAX_ATTR_COUNT = MAX_ATTR_COUNT;
pub const TEST_MAX_NAME_LEN = MAX_NAME_LEN;

/// Test hook: mark installed without spawning the flush thread.
pub fn testSetInstalled(cfg: otlp_config.GrafanaOtlpConfig) void {
    Exporter.testSetInstalled(cfg);
}

/// Test hook: clear installed state and drain the ring.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
}

/// Test hook: run one collect (drain + serialize the buffered spans).
pub fn testCollect(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    return collectSpans(alloc, cfg);
}

test {
    _ = @import("otel_traces_test.zig");
}
