//! Tests for the parent half of the repository-fetch channel (M157 §4): the
//! `repo_fetch_request` frame arm in `child_supervisor_read`.
//!
//! The child half round-trips in `engine/repo_fetch_request.zig`; what these
//! cover is the supervisor's side of the same pipe — that an ask reaches the
//! hook with exactly what the child named, that the reply is framed back on the
//! child's stdin, and that both fail-closed paths (no hook, refused ask) frame a
//! refusal rather than a path.
//!
//! Nothing here spawns git. The hook is the seam, and these prove the seam.

const std = @import("std");
const supervisor = @import("child_supervisor.zig");
const pipe_proto = @import("pipe_proto.zig");
const fetch_req = @import("engine/repo_fetch_request.zig");

const clock = @import("common").clock;
const testing = std.testing;

const SHA1 = "9a078a7b5c1d4e2f60718293a4b5c6d7e8f90123";
const ASK = "{\"repository\":\"acme/payments\",\"commit\":\"" ++ SHA1 ++ "\",\"head\":\"\"}";
const RESULT_FRAME = "{\"exit_ok\":true}";

var noop_ctx: u8 = 0;

fn noopActivity(_: *anyopaque, _: @import("contract").activity.ActivityFrame) void {}
fn noopMemory(_: *anyopaque, _: []const u8) void {}

fn activitySink() supervisor.ActivitySink {
    return .{ .ctx = &noop_ctx, .forward = noopActivity };
}
fn memorySink() supervisor.MemorySink {
    return .{ .ctx = &noop_ctx, .forward = noopMemory };
}

/// Drive one lease's read loop over a pair of pipes: write `frames` as the child
/// would, run `readResult` with `hook`, and return the parsed reply the parent
/// framed back on the child's stdin.
const Exchange = struct {
    response: fetch_req.PipeResponse,
    parsed: std.json.Parsed(fetch_req.PipeResponse),

    fn run(hook: ?supervisor.FetchHook, ask_payload: []const u8) !Exchange {
        const out = try pipe_proto.testOsPipe(); // child→parent stdout
        defer pipe_proto.testOsClose(out[0]);
        const resp = try pipe_proto.testOsPipe(); // parent→child stdin
        defer pipe_proto.testOsClose(resp[0]);

        try pipe_proto.writeFrame(out[1], .repo_fetch_request, ask_payload);
        try pipe_proto.writeFrame(out[1], .result, RESULT_FRAME);
        pipe_proto.testOsClose(out[1]);

        const dl = clock.nowMillis() + 5_000;
        const outcome = try supervisor.readResult(
            testing.allocator,
            out[0],
            resp[1],
            dl,
            activitySink(),
            memorySink(),
            null,
            null,
            hook,
        );
        defer testing.allocator.free(outcome.bytes);
        // The fetch ask never terminates the lease — the run continues to its own
        // result, refused fetch or not.
        try testing.expectEqualStrings(RESULT_FRAME, outcome.bytes);

        pipe_proto.testOsClose(resp[1]);
        const reply = try pipe_proto.readFrame(testing.allocator, resp[0], dl, 4096);
        defer testing.allocator.free(reply.frame.payload);
        try testing.expectEqual(pipe_proto.FrameType.repo_fetch_response, reply.frame.ftype);
        // `alloc_always` because the frame payload is freed by the defer above and
        // the parsed strings outlive this function — the default borrows from the
        // input, which would hand every assertion below a dangling slice.
        const parsed = try std.json.parseFromSlice(
            fetch_req.PipeResponse,
            testing.allocator,
            reply.frame.payload,
            .{ .allocate = .alloc_always },
        );
        return .{ .response = parsed.value, .parsed = parsed };
    }

    fn deinit(self: *Exchange) void {
        self.parsed.deinit();
    }
};

/// Records what the hook was handed, so the ask can be asserted on the far side
/// of the wire rather than trusted.
const Recorder = struct {
    var repository: [64]u8 = @splat(0);
    var repository_len: usize = 0;
    var calls: usize = 0;

    fn reset() void {
        repository_len = 0;
        calls = 0;
    }

    fn onFetch(_: *anyopaque, _: std.mem.Allocator, repo: []const u8, commit: []const u8, head: []const u8, _: ?supervisor.RenewTick) supervisor.FetchOutcome {
        calls += 1;
        @memcpy(repository[0..repo.len], repo);
        repository_len = repo.len;
        testing.expectEqualStrings(SHA1, commit) catch return .{ .refused = "commit mismatch" };
        testing.expectEqualStrings("", head) catch return .{ .refused = "head mismatch" };
        return .{ .ready = "repo" };
    }

    fn hook() supervisor.FetchHook {
        return .{ .ctx = &noop_ctx, .onFetch = onFetch };
    }
};

test "a fetch ask reaches the hook verbatim and its path is framed back" {
    Recorder.reset();
    var ex = try Exchange.run(Recorder.hook(), ASK);
    defer ex.deinit();

    try testing.expectEqual(@as(usize, 1), Recorder.calls);
    // The hook sees exactly what the child named — nothing rewritten in transit.
    try testing.expectEqualStrings("acme/payments", Recorder.repository[0..Recorder.repository_len]);

    try testing.expect(ex.response.ok);
    // A workspace-RELATIVE path: the daemon never tells the sandbox where its
    // workspace lives, because the sandbox has no use for that fact.
    try testing.expectEqualStrings("repo", ex.response.path);
    try testing.expect(!std.fs.path.isAbsolute(ex.response.path));
}

test "a lease with no fetch hook refuses every ask with a reason" {
    // The null-hook case is what a lease that was never granted a repository
    // fetch looks like, and it must be a named refusal rather than silence — the
    // child otherwise cannot tell "not configured" from "still waiting".
    var ex = try Exchange.run(null, ASK);
    defer ex.deinit();
    try testing.expect(!ex.response.ok);
    try testing.expect(ex.response.reason.len > 0);
    try testing.expectEqualStrings("", ex.response.path);
}

test "a malformed ask is refused without the hook ever running" {
    // Garbage on the wire must not reach the code that mints and dials. The
    // parse is the gate, and a hook call here would mean it is not.
    Recorder.reset();
    var ex = try Exchange.run(Recorder.hook(), "{\"repository\":");
    defer ex.deinit();
    try testing.expectEqual(@as(usize, 0), Recorder.calls);
    try testing.expect(!ex.response.ok);
    try testing.expect(ex.response.reason.len > 0);
}

test "test_fetch_is_on_demand_over_the_wire" {
    // Dimension 4.6a's on-demand half, at the transport: a lease that asks for
    // no repository invokes the hook zero times, so it mints nothing, dials
    // nothing, and pays nothing. The fetch is a tool call, never a lease cost.
    Recorder.reset();
    const out = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(out[0]);
    const resp = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(resp[0]);
    defer pipe_proto.testOsClose(resp[1]);

    // A whole lease with no fetch ask in it.
    try pipe_proto.writeFrame(out[1], .activity, "{\"tool_call_started\":{\"name\":\"http_request\",\"args_redacted\":\"{}\"}}");
    try pipe_proto.writeFrame(out[1], .result, RESULT_FRAME);
    pipe_proto.testOsClose(out[1]);

    const outcome = try supervisor.readResult(
        testing.allocator,
        out[0],
        resp[1],
        clock.nowMillis() + 5_000,
        activitySink(),
        memorySink(),
        null,
        null,
        Recorder.hook(),
    );
    defer testing.allocator.free(outcome.bytes);
    try testing.expectEqual(@as(usize, 0), Recorder.calls);
}

test "a refusal the hook raises reaches the child as its own reason" {
    // The reasons are what the model reformulates against, so they must survive
    // the wire intact rather than collapsing into a generic failure.
    const Refuser = struct {
        fn onFetch(_: *anyopaque, _: std.mem.Allocator, _: []const u8, _: []const u8, _: []const u8, _: ?supervisor.RenewTick) supervisor.FetchOutcome {
            return .{ .refused = "repository is outside the fleet's binding" };
        }
    };
    var ex = try Exchange.run(.{ .ctx = &noop_ctx, .onFetch = Refuser.onFetch }, ASK);
    defer ex.deinit();
    try testing.expect(!ex.response.ok);
    try testing.expectEqualStrings("repository is outside the fleet's binding", ex.response.reason);
}
