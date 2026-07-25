//! OpenTelemetry Protocol (OTLP) JSON span exporter for Grafana Cloud Tempo.
//! Callers push completed spans; the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/traces on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate (Ring + Exporter + config + post):
//! this file now carries only the span shape, the span serialization, and the
//! enqueue API. Lifecycle/thread/POST are the shared substrate.

const std = @import("std");
const common = @import("common");
const trace = @import("trace.zig");
const health = @import("metrics_otel.zig");
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

/// OTLP `SpanKind`. Only the two kinds this process actually produces are
/// modelled: HTTP ingress is a server span; the settled delivery observation is
/// internal, because no runner span or trace context exists for it to be the
/// client half of.
pub const SpanKind = enum(u8) {
    internal = 1,
    server = 2,
};

/// A span attribute value. A tagged union rather than a string buffer plus an
/// `is_int` flag so the serializer's switch is exhaustive and a status code can
/// never be emitted as a quoted string.
const AttrValue = union(enum) {
    string: struct { buf: [MAX_ATTR_VAL_LEN]u8, len: u8 },
    int: i64,
};

const SpanAttr = struct {
    key: [MAX_ATTR_KEY_LEN]u8,
    key_len: u8,
    value: AttrValue,
};

/// One completed span, fixed-size and copied by value into the ring. Public so
/// emit sites can name it when they split attribute application into helpers.
pub const SpanEntry = struct {
    trace_id: [trace.TRACE_ID_HEX_LEN]u8,
    span_id: [trace.SPAN_ID_HEX_LEN]u8,
    parent_span_id: [trace.SPAN_ID_HEX_LEN]u8,
    has_parent: bool,
    start_ns: u64,
    end_ns: u64,
    kind: SpanKind,
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
    .signal = .traces,
    .path = OTLP_TRACES_PATH,
    .scope = .otel_traces,
    .collect = collectSpans,
    .pending_count = spansPendingCount,
    .wake_threshold = FLUSH_BATCH_SIZE,
});

pub const install = Exporter.install;
pub const uninstall = Exporter.uninstall;
pub const isInstalled = Exporter.isInstalled;

/// Enqueue a completed span for async export. Non-blocking, fire-and-forget.
pub fn enqueueSpan(entry: SpanEntry) void {
    if (!Exporter.isInstalled()) return;
    if (g_ring.push(entry)) {
        health.setQueueDepth(.traces, g_ring.len());
        Exporter.notifyAccepted();
    } else {
        health.recordDiscard(.traces, .ring_full, 1);
    }
}

/// Helper: build a SpanEntry from a TraceContext, name, kind, and timing.
pub fn buildSpan(
    ctx: trace.TraceContext,
    name: []const u8,
    kind: SpanKind,
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
    entry.kind = kind;
    entry.name_len = @intCast(@min(name.len, MAX_NAME_LEN));
    @memcpy(entry.name[0..entry.name_len], name[0..entry.name_len]);
    entry.attr_count = 0;
    return entry;
}

/// Reserve the next attribute slot and write its key. Returns null when the
/// span is full or the key does not fit — keys are compile-time registry
/// constants, so a rejection here is a registry/bound mismatch, not input.
fn claimAttr(entry: *SpanEntry, key: []const u8) ?*SpanAttr {
    if (entry.attr_count >= MAX_ATTR_COUNT) return null;
    if (key.len > MAX_ATTR_KEY_LEN) return null;
    const slot = &entry.attrs[entry.attr_count];
    slot.key_len = @intCast(key.len);
    @memcpy(slot.key[0..key.len], key);
    entry.attr_count += 1;
    return slot;
}

/// Add a string attribute. Returns false when the span is full or the value
/// exceeds the fixed bound — the attribute is dropped whole rather than
/// truncated, because a half-written identifier reads as a different one.
pub fn addAttr(entry: *SpanEntry, key: []const u8, val: []const u8) bool {
    if (val.len > MAX_ATTR_VAL_LEN) return false;
    const slot = claimAttr(entry, key) orelse return false;
    slot.value = .{ .string = .{
        // SAFETY: only buf[0..len] is ever read, and it is written immediately below.
        .buf = undefined,
        .len = @intCast(val.len),
    } };
    @memcpy(slot.value.string.buf[0..val.len], val);
    return true;
}

/// Add an integer attribute, serialized as an OTLP `intValue`. Used where the
/// pinned conventions define a numeric type (status codes, token counts) so
/// backends can aggregate them instead of parsing strings.
pub fn addIntAttr(entry: *SpanEntry, key: []const u8, val: i64) bool {
    const slot = claimAttr(entry, key) orelse return false;
    slot.value = .{ .int = val };
    return true;
}

// ---------------------------------------------------------------------------
// Serialization (the exporter's collect hook)
// ---------------------------------------------------------------------------

fn spansPendingCount() usize {
    return g_ring.len();
}

fn collectSpans(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    max_entries: usize,
) otlp_exporter.CollectResult {
    if (max_entries == 0) return .empty;
    var removed: usize = 0;
    const body = collectSpansBody(alloc, cfg, max_entries, &removed) catch {
        return .{ .serialize_failed = removed };
    };
    if (removed == 0) return .empty;
    return .{ .ready = .{
        .body = body,
        .removed_count = removed,
        .export_count = removed,
    } };
}

fn collectSpansBody(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    max_entries: usize,
    removed: *usize,
) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    // Shared with logs + metrics so all three envelopes carry byte-identical
    // service identity and the same pinned schema URL.
    try otlp_config.appendEnvelopePrefix(&out, alloc, cfg, "resourceSpans", "scopeSpans", "spans");

    var first = true;
    const limit = @min(max_entries, FLUSH_BATCH_SIZE);
    while (removed.* < limit) {
        const entry = g_ring.pop() orelse break;
        removed.* += 1;
        if (!first) try out.appendSlice(alloc, ",");
        first = false;
        try appendSpan(&out, alloc, entry);
    }

    // Ring drained empty. The envelope prefix is already in `out`, and errdefer
    // does not cover a successful return — free it here or every empty collect
    // strands one buffer.
    if (removed.* == 0) {
        out.deinit(alloc);
        return &.{};
    }

    try out.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);
    return try out.toOwnedSlice(alloc);
}

fn appendSpan(out: *std.ArrayList(u8), alloc: std.mem.Allocator, entry: SpanEntry) !void {
    try out.print(alloc, "{{\"traceId\":\"{s}\",\"spanId\":\"{s}\"", .{ entry.trace_id, entry.span_id });
    if (entry.has_parent) {
        try out.print(alloc, ",\"parentSpanId\":\"{s}\"", .{entry.parent_span_id});
    }
    try out.print(
        alloc,
        ",\"name\":{f},\"kind\":{d},\"startTimeUnixNano\":\"{d}\",\"endTimeUnixNano\":\"{d}\"",
        .{ std.json.fmt(entry.name[0..entry.name_len], .{}), @intFromEnum(entry.kind), entry.start_ns, entry.end_ns },
    );
    if (entry.attr_count > 0) try appendAttributes(out, alloc, entry);
    try out.appendSlice(alloc, "}");
}

fn appendAttributes(out: *std.ArrayList(u8), alloc: std.mem.Allocator, entry: SpanEntry) !void {
    try out.appendSlice(alloc, ",\"attributes\":[");
    for (entry.attrs[0..entry.attr_count], 0..) |attr, attr_index| {
        if (attr_index > 0) try out.appendSlice(alloc, ",");
        try out.print(alloc, "{{\"key\":{f},\"value\":", .{std.json.fmt(attr.key[0..attr.key_len], .{})});
        switch (attr.value) {
            // json.fmt supplies the quotes and escapes the interior.
            .string => |s| try out.print(alloc, "{{\"stringValue\":{f}}}", .{std.json.fmt(s.buf[0..s.len], .{})}),
            // OTLP carries 64-bit ints as JSON strings to survive consumers
            // whose number type is a double.
            .int => |v| try out.print(alloc, "{{\"intValue\":\"{d}\"}}", .{v}),
        }
        try out.appendSlice(alloc, "}");
    }
    try out.appendSlice(alloc, "]");
}

pub const TestRing = RingT;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;
pub const TEST_MAX_ATTR_COUNT = MAX_ATTR_COUNT;
pub const TEST_MAX_NAME_LEN = MAX_NAME_LEN;

/// Test hook: mark installed without spawning the flush thread.
pub fn testSetInstalled(cfg: otlp_config.GrafanaOtlpConfig) void {
    Exporter.testSetInstalled(common.globalIo(), cfg);
}

/// Test hook: clear installed state and drain the ring.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
    health.setQueueDepth(.traces, 0);
}

/// Test hook: run one collect (drain + serialize the buffered spans).
pub fn testCollect(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    return switch (collectSpans(alloc, cfg, FLUSH_BATCH_SIZE)) {
        .empty => null,
        .ready => |batch| batch.body,
        .serialize_failed => error.SerializationFailed,
    };
}

/// Test hook: number of entries pending in the production ring.
pub fn testPendingCount() usize {
    return spansPendingCount();
}

/// Test hook: accepted pushes counted toward the next exporter cycle.
pub fn testAcceptedSinceCycle() u32 {
    return Exporter.testAcceptedSinceCycle();
}

test {
    _ = @import("otel_traces_test.zig");
}
