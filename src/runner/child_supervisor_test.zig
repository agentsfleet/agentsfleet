//! Tests for child_supervisor's stdout read path and fail-closed sandbox setup.
//! Extracted to a sibling so the supervisor file stays within the line budget;
//! consumes `readResult` + `establishSandbox` as the in-tree test consumer.

const std = @import("std");
const builtin = @import("builtin");
const supervisor = @import("child_supervisor.zig");
const pipe_proto = @import("pipe_proto.zig");
const client_errors = @import("engine/client_errors.zig");
const contract = @import("contract");

const ActivityFrame = contract.activity.ActivityFrame;
const ActivitySink = supervisor.ActivitySink;

test "readResult forwards activity frames in order and returns the result frame" {
    const Cap = struct {
        count: usize = 0,
        name_buf: [64]u8 = [_]u8{0} ** 64,
        name_len: usize = 0,
        fn forward(ctx: *anyopaque, frame: ActivityFrame) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.count += 1;
            if (frame == .tool_call_started) {
                const n = frame.tool_call_started.name;
                @memcpy(self.name_buf[0..n.len], n);
                self.name_len = n.len;
            }
        }
    };

    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    const af = ActivityFrame{ .tool_call_started = .{ .name = "fly_deploy", .args_redacted = "{}" } };
    const af_json = try std.json.Stringify.valueAlloc(std.testing.allocator, af, .{});
    defer std.testing.allocator.free(af_json);
    try pipe_proto.writeFrame(fds[1], .activity, af_json);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    std.posix.close(fds[1]);

    var cap: Cap = .{};
    const sink = ActivitySink{ .ctx = &cap, .forward = Cap.forward };
    const dl = std.time.milliTimestamp() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], dl, sink, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(!outcome.timed_out);
    try std.testing.expect(!outcome.terminated);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
    try std.testing.expectEqual(@as(usize, 1), cap.count);
    try std.testing.expectEqualStrings("fly_deploy", cap.name_buf[0..cap.name_len]);
}

test "readResult tolerates a malformed activity frame and still returns the result" {
    const Noop = struct {
        fn forward(_: *anyopaque, _: ActivityFrame) void {}
    };
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    try pipe_proto.writeFrame(fds[1], .activity, "{not valid json"); // dropped
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":false}");
    std.posix.close(fds[1]);

    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = Noop.forward };
    const dl = std.time.milliTimestamp() + 5_000;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], dl, sink, null);
    defer std.testing.allocator.free(outcome.bytes);
    try std.testing.expectEqualStrings("{\"exit_ok\":false}", outcome.bytes);
}

// A scripted renewal hook: returns a queued decision per tick so a test can
// drive readResult's renewal path deterministically without any HTTP.
const ScriptedHook = struct {
    decisions: []const supervisor.RenewDecision,
    idx: usize = 0,
    ticks: usize = 0,
    fn onTick(ctx: *anyopaque, now_ms: i64) supervisor.RenewDecision {
        _ = now_ms;
        const self: *ScriptedHook = @ptrCast(@alignCast(ctx));
        self.ticks += 1;
        if (self.idx >= self.decisions.len) return .keep;
        const d = self.decisions[self.idx];
        self.idx += 1;
        return d;
    }
};

const NoopSink = struct {
    fn forward(_: *anyopaque, _: ActivityFrame) void {}
};

test "readResult: a hook returning .terminate kills the wait and reports terminated" {
    // A pipe with no data and an open write end: the read blocks, so the tick
    // fires. The scripted hook terminates on the first tick. A far deadline
    // proves termination came from the hook, not a timeout.
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    defer std.posix.close(fds[1]); // keep write end open → no EOF, forces a tick

    var hook_state = ScriptedHook{ .decisions = &.{.terminate} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const dl = std.time.milliTimestamp() + 60_000; // far; the 10ms tick fires first
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], dl, sink, hook);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(outcome.terminated);
    try std.testing.expect(!outcome.timed_out);
    try std.testing.expect(hook_state.ticks >= 1);
}

test "readResult: a hook .extend past a near deadline keeps reading to the result" {
    // The lease deadline is near (100ms); without renewal the wait would time
    // out before the writer (200ms) sends the result. The hook extends on the
    // first 10ms tick, so the result is still read cleanly.
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);

    var hook_state = ScriptedHook{ .decisions = &.{.{ .extend = std.time.milliTimestamp() + 60_000 }} };
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = ScriptedHook.onTick, .tick_ms = 10 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const Writer = struct {
        fn run(write_fd: std.posix.fd_t) void {
            std.Thread.sleep(200 * std.time.ns_per_ms);
            pipe_proto.writeFrame(write_fd, .result, "{\"exit_ok\":true}") catch {};
            std.posix.close(write_fd);
        }
    };
    var wt = try std.Thread.spawn(.{}, Writer.run, .{fds[1]});
    defer wt.join();

    const near_dl = std.time.milliTimestamp() + 100;
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], near_dl, sink, hook);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(!outcome.timed_out);
    try std.testing.expect(!outcome.terminated);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
}

test "sandbox setup fails closed: dev_none runs bare, a required tier with no domain refuses" {
    // dev_none = explicit no-isolation. Every other tier MUST establish its
    // domain or the lease is refused unrun (never executed unsandboxed). The
    // non-Linux arm is here; the Linux cgroup-failure arm is in test-integration.
    try std.testing.expect((try supervisor.establishSandbox(std.testing.allocator, false)) == null);
    if (builtin.os.tag != .linux)
        try std.testing.expectError(error.SandboxUnavailable, supervisor.establishSandbox(std.testing.allocator, true));
    try std.testing.expect(client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED.len > 0);
}
