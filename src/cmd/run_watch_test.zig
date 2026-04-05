//! M22_001 §1 — unit tests for SseState and renderSseEvent.
//!
//! Covers: SSE line parsing, event dispatch, done flag, edge cases,
//! malformed data, memory safety, CRLF handling, partial chunks.

const std = @import("std");
const run_watch = @import("run_watch.zig");

// ---------------------------------------------------------------------------
// T1 — Happy path: SseState parsing
// ---------------------------------------------------------------------------

test "T1: feedBytes parses a complete gate_result event" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "event: gate_result\ndata: {\"gate_name\":\"lint\",\"outcome\":\"PASS\",\"loop\":1,\"wall_ms\":100}\n\n";
    sse.feedBytes(input);

    // After the blank line, current_data should be cleared (dispatched).
    try std.testing.expect(sse.current_data == null);
    try std.testing.expect(sse.current_event == null);
    try std.testing.expect(!sse.done);
}

test "T1: feedBytes sets done=true on run_complete" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(sse.done);
}

test "T1: feedBytes handles multiple events in sequence" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "event: gate_result\ndata: {\"gate_name\":\"lint\"}\n\nevent: gate_result\ndata: {\"gate_name\":\"test\"}\n\nevent: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(sse.done);
}

test "T1: feedBytes with default event type (no event: line)" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    // No "event:" line — defaults to "message", which renderSseEvent ignores.
    const input = "data: hello\n\n";
    sse.feedBytes(input);

    try std.testing.expect(!sse.done);
    try std.testing.expect(sse.current_data == null);
}

test "T1: feedBytes extracts data correctly" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    // Feed event: and data: lines without blank line — data should be buffered.
    sse.feedBytes("event: gate_result\n");
    sse.feedBytes("data: {\"gate_name\":\"build\"}\n");

    // Before blank line, data is still buffered.
    try std.testing.expect(sse.current_data != null);
    try std.testing.expectEqualStrings("{\"gate_name\":\"build\"}", sse.current_data.?);
    try std.testing.expectEqualStrings("gate_result", sse.current_event.?);
}

// ---------------------------------------------------------------------------
// T2 — Edge cases
// ---------------------------------------------------------------------------

test "T2: feedBytes with empty input" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    sse.feedBytes("");
    try std.testing.expect(!sse.done);
    try std.testing.expect(sse.current_data == null);
}

test "T2: feedBytes with only newlines (blank lines)" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    sse.feedBytes("\n\n\n");
    try std.testing.expect(!sse.done);
}

test "T2: feedBytes with CRLF line endings" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "event: run_complete\r\ndata: {\"state\":\"DONE\"}\r\n\r\n";
    sse.feedBytes(input);

    try std.testing.expect(sse.done);
}

test "T2: feedBytes with data split across multiple chunks" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    // Simulate chunked delivery.
    sse.feedBytes("event: run_");
    sse.feedBytes("complete\n");
    sse.feedBytes("data: {\"state\":");
    sse.feedBytes("\"DONE\"}\n");
    sse.feedBytes("\n");

    try std.testing.expect(sse.done);
}

test "T2: feedBytes with comment lines (colon prefix)" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    // SSE spec: lines starting with : are comments — should be ignored.
    const input = ": heartbeat\n\nevent: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(sse.done);
}

test "T2: feedBytes with id: line does not affect state" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "id: 1700000000001\nevent: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(sse.done);
}

test "T2: second data: line overwrites first" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    sse.feedBytes("data: first\n");
    sse.feedBytes("data: second\n");

    try std.testing.expectEqualStrings("second", sse.current_data.?);
}

// ---------------------------------------------------------------------------
// T3 — Error paths
// ---------------------------------------------------------------------------

test "T3: feedBytes with malformed JSON in data does not crash" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    // renderSseEvent will try to parse and silently fail.
    const input = "event: gate_result\ndata: {INVALID JSON\n\n";
    sse.feedBytes(input);

    try std.testing.expect(!sse.done);
}

test "T3: feedBytes with data but no event line dispatches as message (ignored)" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "data: some data\n\n";
    sse.feedBytes(input);

    // "message" events are not gate_result or run_complete — renderSseEvent ignores them.
    try std.testing.expect(!sse.done);
}

test "T3: feedBytes with blank line before any fields is a no-op" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "\nevent: gate_result\ndata: {}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(!sse.done);
}

test "T3: feedBytes with unknown event type does not crash" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    const input = "event: unknown_type\ndata: {}\n\n";
    sse.feedBytes(input);

    try std.testing.expect(!sse.done);
}

test "T3: feedBytes with very long data line does not crash" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();

    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(alloc);
    const w = buf.writer(alloc);
    try w.writeAll("data: ");
    for (0..10000) |_| try w.writeByte('x');
    try w.writeByte('\n');
    try w.writeByte('\n');

    sse.feedBytes(buf.items);
    try std.testing.expect(!sse.done);
}

// ---------------------------------------------------------------------------
// T11 — Memory safety
// ---------------------------------------------------------------------------

test "T11: SseState init/deinit has no leaks" {
    const alloc = std.testing.allocator;
    for (0..100) |_| {
        var sse = run_watch.SseState.init(alloc);
        sse.feedBytes("event: gate_result\ndata: {\"gate_name\":\"lint\"}\n\n");
        sse.deinit();
    }
}

test "T11: SseState with partial data then deinit has no leaks" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    // Feed data but no blank line — data is buffered, not dispatched.
    sse.feedBytes("event: gate_result\ndata: {\"gate_name\":\"lint\"}\n");
    // deinit must free the buffered data without leaking.
    sse.deinit();
}

test "T11: SseState overwriting data multiple times has no leaks" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();
    for (0..50) |_| {
        sse.feedBytes("data: first\n");
        sse.feedBytes("data: second\n");
        sse.feedBytes("data: third\n");
        sse.feedBytes("\n");
    }
}

test "T11: SseState feedBytes with run_complete then more data has no leaks" {
    const alloc = std.testing.allocator;
    var sse = run_watch.SseState.init(alloc);
    defer sse.deinit();
    sse.feedBytes("event: run_complete\ndata: {\"state\":\"DONE\"}\n\n");
    try std.testing.expect(sse.done);
    // Feed more data after done — should still process cleanly.
    sse.feedBytes("event: gate_result\ndata: {\"gate_name\":\"extra\"}\n\n");
}

test "T11: SseState repeated init/feedBytes/deinit cycles" {
    const alloc = std.testing.allocator;
    for (0..20) |_| {
        var sse = run_watch.SseState.init(alloc);
        sse.feedBytes("event: gate_result\ndata: {}\n\nevent: run_complete\ndata: {}\n\n");
        sse.deinit();
    }
}
