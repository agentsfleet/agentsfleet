//! Edge tests for child_supervisor's read loop deadline handling. Mirrors the
//! sibling test's real-pipe + scripted-hook harness. Pins the fail-safe contract
//! that a renewal hook which "extends" to an ALREADY-PAST deadline cannot wedge
//! the loop: the very next wait sees the elapsed deadline and the supervisor
//! reports timed_out rather than spinning forever.

const std = @import("std");
const clock = @import("common").clock;
const supervisor = @import("child_supervisor.zig");
const pipe_proto = @import("pipe_proto.zig");
const contract = @import("contract");
const cgroup = @import("engine/CgroupScope.zig");

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

// A hook that, on its first tick, "extends" the deadline to a value already in
// the past — modelling a server clock skew / stale renewal. The loop must detect
// the elapsed deadline next iteration and exit, not loop.
const PastExtendHook = struct {
    ticks: usize = 0,
    fn onTick(ctx: *anyopaque, now_ms: i64, _: supervisor.UsageSnapshot) supervisor.RenewDecision {
        const self: *PastExtendHook = @ptrCast(@alignCast(ctx));
        self.ticks += 1;
        return .{ .extend = now_ms - std.time.ms_per_s }; // already past (1s ago)
    }
};

const NoopSink = struct {
    fn forward(_: *anyopaque, _: ActivityFrame) void {}
};

test "readResult should exit cleanly when the hook extends to an already-past deadline" {
    // A pipe with no data and an open write end: the read blocks, the short tick
    // fires, the hook "extends" to a past instant. The next wait sees the elapsed
    // deadline → timed_out, loop exits. A far initial deadline proves the timeout
    // came from the past-extend, not the original lease window.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    defer pipe_proto.testOsClose(fds[1]); // open write end → no EOF, forces ticks

    var hook_state = PastExtendHook{};
    const hook = supervisor.RenewHook{ .ctx = &hook_state, .onTick = PastExtendHook.onTick, .tick_ms = 10 };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = NoopSink.forward };

    const far_dl = clock.nowMillis() + 60_000; // far; the 10ms tick fires first
    const outcome = try supervisor.readResult(std.testing.allocator, fds[0], fds[1], far_dl, sink, NoopMem.sink(), hook, null);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(outcome.timed_out);
    try std.testing.expect(!outcome.terminated);
    try std.testing.expect(hook_state.ticks >= 1);
}

test "enrollOrFail is a no-op success for a null (dev_none) scope and never signals a pid" {
    // dev_none establishes no cgroup, so the scope is null. enrollOrFail must
    // return cleanly — the lease is NOT refused — and must take the early-return
    // branch BEFORE killChild, so no `kill(-pid)` is ever issued. The pid below is
    // never spawned and never signalled precisely because that branch returns
    // first; a regression that drops the null guard would fail here (or kill an
    // unrelated process). Guards the local-dev path against an accidental refusal.
    var scope: ?cgroup = null;
    try supervisor.enrollOrFail(&scope, 999_999, "dev-none-lease");
    try std.testing.expect(scope == null); // nothing enrolled, scope untouched
}

/// A mint hook that refuses every ask — the fail-closed default a lease runs
/// under when the control plane grants the fleet no integration.
const RejectingMint = struct {
    var dummy: u8 = 0;
    fn onMint(_: *anyopaque, _: std.mem.Allocator, _: []const u8, _: ?[]const u8) supervisor.CredentialOutcome {
        return .rejected;
    }
    fn hook() supervisor.MintHook {
        return .{ .ctx = &dummy, .onMint = onMint };
    }
};

fn readOnce(fd: std.posix.fd_t, response_fd: std.posix.fd_t, deadline_ms: i64, mint_hook: ?supervisor.MintHook) !supervisor.ReadOutcome {
    return supervisor.readResult(
        std.testing.allocator,
        fd,
        response_fd,
        deadline_ms,
        .{ .ctx = &NoopMem.dummy, .forward = NoopSink.forward },
        NoopMem.sink(),
        null,
        mint_hook,
    );
}

test "a child that closes its stdout without a result frame ends the read cleanly" {
    // The normal end of a crashed child: the pipe goes readable at EOF, and the
    // loop must return an empty outcome instead of treating a closed stream as
    // a timeout — the supervisor classifies the exit from the wait status, and a
    // spurious timeout would relabel a crash as a deadline kill.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    pipe_proto.testOsClose(fds[1]); // EOF now

    const out = try readOnce(fds[0], fds[0], clock.nowMillis() + 5_000, null);
    try std.testing.expect(!out.timed_out);
    try std.testing.expect(out.bytes.len == 0);
}

test "a parent-only frame arriving on the child's stdout is dropped, not obeyed" {
    // `lease` and `credential_response` are parent to child only. One coming back
    // up the child's stdout is wire skew or a confused child; the parent logs it
    // and keeps reading rather than acting on a frame it never sends upward.
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);

    try pipe_proto.writeFrame(fds[1], .lease, "{}");
    try pipe_proto.writeFrame(fds[1], .result, "done");
    pipe_proto.testOsClose(fds[1]);

    const out = try readOnce(fds[0], fds[0], clock.nowMillis() + 5_000, null);
    defer std.testing.allocator.free(out.bytes);
    // The skewed frame was skipped and the real result still arrived.
    try std.testing.expectEqualStrings("done", out.bytes);
}

test "a refused mint is answered on the wire, so the child fails closed instead of hanging" {
    // The child blocks reading its reply. A rejection that is never framed back
    // leaves it stuck until the lease deadline — so refusing must still answer.
    const child_out = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(child_out[0]);
    const child_in = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(child_in[0]);
    defer pipe_proto.testOsClose(child_in[1]);

    try pipe_proto.writeFrame(child_out[1], .credential_request, "{\"integration\":\"github\"}");
    try pipe_proto.writeFrame(child_out[1], .result, "done");
    pipe_proto.testOsClose(child_out[1]);

    const out = try readOnce(child_out[0], child_in[1], clock.nowMillis() + 5_000, RejectingMint.hook());
    defer std.testing.allocator.free(out.bytes);
    try std.testing.expectEqualStrings("done", out.bytes);

    const reply = try pipe_proto.readFrame(std.testing.allocator, child_in[0], clock.nowMillis() + 5_000, 4096);
    try std.testing.expect(reply == .frame);
    defer std.testing.allocator.free(reply.frame.payload);
    try std.testing.expect(std.mem.indexOf(u8, reply.frame.payload, "\"ok\":false") != null);
}
