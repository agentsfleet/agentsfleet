const std = @import("std");
const trace = @import("trace.zig");
const otel_traces = @import("otel_traces.zig");
const otlp_config = @import("otlp/config.zig");
const health = @import("metrics_otel.zig");

const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};

const Ring = otel_traces.TestRing;
const BUFFER_CAPACITY = otel_traces.TEST_BUFFER_CAPACITY;
const MAX_ATTR_COUNT = otel_traces.TEST_MAX_ATTR_COUNT;
const MAX_NAME_LEN = otel_traces.TEST_MAX_NAME_LEN;
const buildSpan = otel_traces.buildSpan;
const addAttr = otel_traces.addAttr;
const enqueueSpan = otel_traces.enqueueSpan;

test "ring buffer push and pop round-trip" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const ctx = trace.TraceContext.generate();
    // pin test: literal is the contract
    var entry = buildSpan(ctx, "test.span", 1000, 2000);
    try std.testing.expect(addAttr(&entry, "key1", "val1"));

    try std.testing.expect(ring.push(entry));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    // pin test: literal is the contract
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

// Encodes (thread, seq) into the span timing pair and name so a torn entry
// (fields mixed from two writers) is detectable on pop.
const ENCODE_BASE: u64 = 1_000_000;
const NAME_FMT = "t{d}.s{d}";

test "ring buffer concurrent pushes never tear, lose, or duplicate entries" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const THREADS: usize = 4;
    const PER_THREAD: usize = 200; // 800 total < capacity-1 → zero drops expected

    const Pusher = struct {
        fn run(r: *Ring, thread_idx: usize) void {
            const ctx = trace.TraceContext.generate();
            var seq: usize = 0;
            while (seq < PER_THREAD) : (seq += 1) {
                var name_buf: [32]u8 = undefined;
                const name = std.fmt.bufPrint(&name_buf, NAME_FMT, .{ thread_idx, seq }) catch unreachable;
                const encoded = @as(u64, thread_idx) * ENCODE_BASE + seq;
                const entry = buildSpan(ctx, name, encoded, encoded + 1);
                _ = r.push(entry);
            }
        }
    };

    var threads: [THREADS]std.Thread = undefined;
    for (&threads, 0..) |*t, idx| t.* = try std.Thread.spawn(.{}, Pusher.run, .{ ring, idx });
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u64, 0), ring.dropped.load(.acquire));

    var seen = [_]bool{false} ** (THREADS * PER_THREAD);
    var popped_count: usize = 0;
    while (ring.pop()) |entry| {
        popped_count += 1;
        const thread_idx: usize = @intCast(entry.start_ns / ENCODE_BASE);
        const seq: usize = @intCast(entry.start_ns % ENCODE_BASE);
        // Cross-field consistency: a torn slot mixes (thread, seq) across fields.
        try std.testing.expectEqual(entry.start_ns + 1, entry.end_ns);
        var name_buf: [32]u8 = undefined;
        const want_name = std.fmt.bufPrint(&name_buf, NAME_FMT, .{ thread_idx, seq }) catch unreachable;
        try std.testing.expectEqualStrings(want_name, entry.name[0..entry.name_len]);
        const key = thread_idx * PER_THREAD + seq;
        try std.testing.expect(!seen[key]); // duplicate delivery
        seen[key] = true;
    }
    try std.testing.expectEqual(THREADS * PER_THREAD, popped_count); // lost entry
}

test "pop returns null on a claimed-but-unready head slot, then delivers once published" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    // Simulate a producer that claimed slot 0 (head advanced) but has not yet
    // published its write: head=1, ready[0]=0.
    ring.head.store(1, .release);
    try std.testing.expect(ring.pop() == null);

    const ctx = trace.TraceContext.generate();
    const entry = buildSpan(ctx, "late.publish", 7, 8);
    ring.buffer[0] = entry;
    ring.ready[0].store(1, .release);

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(u64, 7), popped.?.start_ns);
}

test "enqueueSpan is no-op when exporter not installed" {
    const ctx = trace.TraceContext.generate();
    const entry = buildSpan(ctx, "noop", 0, 0);
    enqueueSpan(entry);
}

test "test_traces_full_ring_records_discard_and_only_accepted_wake" {
    health.resetForTest();
    defer health.resetForTest();
    otel_traces.testSetInstalled(TEST_CFG);
    defer otel_traces.testClear();

    const entry = buildSpan(trace.TraceContext.generate(), "selected", 0, 1);
    for (0..BUFFER_CAPACITY - 1) |_| enqueueSpan(entry);
    enqueueSpan(entry);

    const snapshot = health.snapshot();
    try std.testing.expectEqual(@as(usize, BUFFER_CAPACITY - 1), otel_traces.testPendingCount());
    try std.testing.expectEqual(@as(u32, BUFFER_CAPACITY - 1), otel_traces.testAcceptedSinceCycle());
    try std.testing.expectEqual(
        @as(u64, 1),
        snapshot.discarded[@intFromEnum(health.Signal.traces)][@intFromEnum(health.DiscardReason.ring_full)],
    );
}

test "collectSpans serializes valid single-quoted JSON (json.fmt double-quote regression)" {
    const alloc = std.testing.allocator;
    otel_traces.testSetInstalled(TEST_CFG);
    defer otel_traces.testClear();

    const ctx = trace.TraceContext.generate();
    var span = otel_traces.buildSpan(ctx, "run.execute", 100, 200);
    try std.testing.expect(otel_traces.addAttr(&span, "model", "claude-opus-4-8"));
    otel_traces.enqueueSpan(span);

    const body = (try otel_traces.testCollect(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);

    // The prior bug wrapped json.fmt (which already quotes) in its own quotes,
    // emitting ""value"" — invalid JSON that Tempo rejects. A successful parse is
    // the regression assertion; the bug makes the body unparseable.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
    // And the attribute value + span name are single-quoted (the fixed shape).
    try std.testing.expect(std.mem.indexOf(u8, body, "\"stringValue\":\"claude-opus-4-8\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"run.execute\"") != null);
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

test "child span shares parent trace_id (Tempo waterfall invariant)" {
    const root = trace.TraceContext.generate();
    const child = root.child();

    const root_span = buildSpan(root, "run.execute", 100, 200);
    const child_span = buildSpan(child, "fleet.call", 110, 190);

    // Both spans must have the same trace_id.
    try std.testing.expectEqualSlices(u8, &root_span.trace_id, &child_span.trace_id);
    // Child must reference root as parent.
    try std.testing.expect(child_span.has_parent);
    try std.testing.expectEqualSlices(u8, &root_span.span_id, &child_span.parent_span_id);
    // Root must NOT have a parent.
    try std.testing.expect(!root_span.has_parent);
}

test "manual TraceContext with external trace_id produces aligned spans" {
    // Simulate what the root span defer block does:
    // use ctx.trace_id (external) + root_tc.span_id (generated).
    const external_trace_id = "abcdef0123456789abcdef0123456789";
    const root_tc = trace.TraceContext.generate();

    var manual_tc: trace.TraceContext = undefined;
    @memcpy(&manual_tc.trace_id, external_trace_id);
    manual_tc.span_id = root_tc.span_id;
    manual_tc.parent_span_id = null;

    const root_span = buildSpan(manual_tc, "run.execute", 0, 100);

    // Now build a child the way emitFleetSpan does:
    var child_tc: trace.TraceContext = undefined;
    @memcpy(&child_tc.trace_id, external_trace_id);
    const child_gen = trace.TraceContext.generate();
    child_tc.span_id = child_gen.span_id;
    child_tc.parent_span_id = root_tc.span_id;

    const child_span = buildSpan(child_tc, "fleet.call", 10, 90);

    // Invariant: same trace_id, child points to root.
    try std.testing.expectEqualSlices(u8, &root_span.trace_id, &child_span.trace_id);
    try std.testing.expectEqualSlices(u8, &root_span.span_id, &child_span.parent_span_id);
    try std.testing.expectEqualStrings(external_trace_id, &root_span.trace_id);
}

test "collectSpans frees its envelope when the ring drains empty" {
    const alloc = std.testing.allocator;
    otel_traces.testSetInstalled(TEST_CFG);
    defer otel_traces.testClear();

    // No spans pending: collect writes the resourceSpans prefix, finds nothing
    // to serialize, and returns empty. The testing allocator's leak check is the
    // assertion — the early return must release that prefix, and errdefer does
    // not fire on a successful return.
    try std.testing.expect((try otel_traces.testCollect(alloc, TEST_CFG)) == null);
}
