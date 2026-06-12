//! Concurrency-shaped tests for child_supervisor's read loop: activity frames
//! stream in while the renewal hook fires per-frame, and at some point the hook
//! requests .terminate. Mirrors the sibling test's real-pipe + scripted-hook +
//! ActivitySink harness. Pins ordering + the terminate boundary: every frame
//! forwarded BEFORE the terminating tick is delivered to the sink in order, the
//! outcome is `terminated`, and no frame queued AFTER termination is processed.

const std = @import("std");
const clock = @import("common").clock;
const supervisor = @import("child_supervisor.zig");
const pipe_proto = @import("pipe_proto.zig");
const contract = @import("contract");

const ActivityFrame = contract.activity.ActivityFrame;
const ActivitySink = supervisor.ActivitySink;

// No-op memory sink — capture forwarding is out of scope for these read-loop tests.
const NoopMem = struct {
    var dummy: u8 = 0;
    fn forward(_: *anyopaque, _: []const u8) void {}
    fn sink() supervisor.MemorySink {
        return .{ .ctx = &dummy, .forward = forward };
    }
};

// Records the ordered sequence of forwarded activity-frame tool names so a test
// can assert both COUNT and ORDER of delivery up to the terminate boundary.
const OrderedCap = struct {
    // SAFETY: only names[i][0..lens[i]] is ever read; lens starts at 0 and is
    // set on each write, so no uninitialized byte is ever observed.
    names: [16][32]u8 = undefined,
    lens: [16]usize = [_]usize{0} ** 16,
    count: usize = 0,
    fn forward(ctx: *anyopaque, frame: ActivityFrame) void {
        const self: *OrderedCap = @ptrCast(@alignCast(ctx));
        if (self.count >= self.names.len) return;
        if (frame == .tool_call_started) {
            const n = frame.tool_call_started.name;
            const m = @min(n.len, self.names[self.count].len);
            @memcpy(self.names[self.count][0..m], n[0..m]);
            self.lens[self.count] = m;
        }
        self.count += 1;
    }
};

// onTick returns .keep for the first `keep_ticks` calls then .terminate, so the
// terminating tick lands AFTER a known number of forwarded frames. Counts ticks
// so a test can prove the loop stopped calling once it terminated.
const TerminateAfterHook = struct {
    keep_ticks: usize,
    ticks: usize = 0,
    fn onTick(ctx: *anyopaque, now_ms: i64, _: supervisor.UsageSnapshot) supervisor.RenewDecision {
        _ = now_ms;
        const self: *TerminateAfterHook = @ptrCast(@alignCast(ctx));
        self.ticks += 1;
        return if (self.ticks <= self.keep_ticks) .keep else .terminate;
    }
};

fn writeToolCall(write_fd: std.posix.fd_t, name: []const u8) !void {
    const af = ActivityFrame{ .tool_call_started = .{ .name = name, .args_redacted = "{}" } };
    const json = try std.json.Stringify.valueAlloc(std.testing.allocator, af, .{});
    defer std.testing.allocator.free(json);
    try pipe_proto.writeFrame(write_fd, .activity, json);
}

test "readResult should forward frames in order then honor terminate without processing later frames" {
    // Three activity frames are queued, then a result frame that must NEVER be
    // read. The hook keeps for two ticks and terminates on the third: applyTick
    // runs after each forwarded frame, so frames 1–3 are delivered in order and
    // the terminate fires right after frame 3 — frame 4 (the result) is left
    // unread in the pipe. A far deadline proves the stop came from the hook.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    defer pipe_proto.testOsClose(fds[1]); // keep write end open → no spurious EOF

    try writeToolCall(fds[1], "alpha");
    try writeToolCall(fds[1], "bravo");
    try writeToolCall(fds[1], "charlie");
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}"); // must stay unread

    var hook_state = TerminateAfterHook{ .keep_ticks = 2 };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = TerminateAfterHook.onTick, .tick_ms = 10 };
    var cap: OrderedCap = .{};
    const sink = ActivitySink{ .ctx = &cap, .forward = OrderedCap.forward };

    const far_dl = clock.nowMillis() + 60_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], far_dl, sink, NoopMem.sink(), hook);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(outcome.terminated);
    try std.testing.expect(!outcome.timed_out);
    try std.testing.expectEqualStrings("", outcome.bytes); // result frame never reached

    // Exactly the three pre-terminate frames, in order.
    try std.testing.expectEqual(@as(usize, 3), cap.count);
    try std.testing.expectEqualStrings("alpha", cap.names[0][0..cap.lens[0]]);
    try std.testing.expectEqualStrings("bravo", cap.names[1][0..cap.lens[1]]);
    try std.testing.expectEqualStrings("charlie", cap.names[2][0..cap.lens[2]]);
    // The terminating tick is the third per-frame call; no ticks after terminate.
    try std.testing.expectEqual(@as(usize, 3), hook_state.ticks);
}
