//! repo_fetch_request.zig — the child→runner on-demand repository-fetch channel
//! (M157 §4).
//!
//! Sibling of `credential_request.zig`, deliberately line for line: the child
//! holds no token, no control-plane URL, and no route out of its network
//! namespace, so it cannot fetch a repository itself. It asks its runner — a
//! `repo_fetch_request` frame up the stdout pipe, then it blocks reading the
//! `repo_fetch_response` back down stdin. The parent services the ask inline:
//! validates the repository against the lease's binding, mints, fetches, and
//! answers with a workspace-relative path.
//!
//! What the child names and what it does NOT name is the whole design. It names
//! the repository, the commit, and the target branch — because only the model
//! knows which repair it is proposing. It does not name a workspace, a path, or
//! a credential: the daemon derives the workspace from `lease_id` (Invariant 2),
//! the repository is checked against the fleet's declared binding before any
//! network call, and no token crosses this boundary in either direction
//! (Invariant 9). A prompt-injected child can therefore ask for the wrong
//! repository and be refused; it cannot ask for the wrong *place*.
//!
//! The reply is a RELATIVE path for the same reason: an absolute one would be
//! the daemon telling the child where its workspace is, which is a fact the
//! sandbox otherwise never has to state.
//!
//! Fail closed: any transport loss, protocol skew, or typed refusal surfaces as
//! an error, and the tool reports it rather than proceeding against a tree that
//! may not be there.

const std = @import("std");
const pipe_proto = @import("../pipe_proto.zig");
const credential_request = @import("credential_request.zig");

/// child→parent fetch ask (`repo_fetch_request` frame payload). Every field is
/// model-authored text; `repo_fetch.decide` treats all of it as hostile.
pub const PipeRequest = struct {
    /// Full `owner/repo`. Checked against the lease's binding parent-side.
    repository: []const u8,
    /// The suspect commit to revert, as a full object id.
    commit: []const u8,
    /// The branch the revert targets. Empty asks for the remote's default head.
    head: []const u8 = "",
};

/// parent→child fetch reply (`repo_fetch_response` frame payload). `ok` gates
/// the path: a refusal rides as `ok=false` with a human-readable `reason` the
/// model can reformulate against, and the tool fails the call closed.
pub const PipeResponse = struct {
    ok: bool,
    /// Workspace-relative path to the ready working tree (e.g. `repo`).
    path: []const u8 = "",
    /// Why the fetch was refused, when `ok` is false.
    reason: []const u8 = "",
};

/// The child's two pipe ends + the lease wall-clock bound. Same shape as the
/// mint channel's, and for the same reason: the child is single-threaded during
/// a turn, so the ask is the only frame in flight while it blocks for the reply.
pub const Channel = struct {
    request_fd: std.posix.fd_t,
    response_fd: std.posix.fd_t,
    /// Absolute epoch-ms deadline (the lease's `lease_expires_at`).
    deadline_ms: i64,
};

/// The fetch channel for a lease, derived from its mint channel.
///
/// There is ONE child↔runner duplex — the child's stdout and stdin — and
/// `pipe_proto` multiplexes it by frame type. Deriving the fetch channel from
/// the mint channel rather than threading a second one through the five layers
/// between `child_exec` and the tool bridge keeps a single source for the two
/// descriptors and the deadline, so the two channels cannot drift apart.
pub fn channelFrom(mint: credential_request.Channel) Channel {
    return .{
        .request_fd = mint.request_fd,
        .response_fd = mint.response_fd,
        .deadline_ms = mint.deadline_ms,
    };
}

pub const FetchError = error{
    /// Could not write the request frame (parent closed stdout-read end).
    ChannelWrite,
    /// Parent closed the response pipe at a frame boundary before replying.
    ChannelClosed,
    /// The lease deadline elapsed mid round-trip.
    FetchTimeout,
    /// A non-`repo_fetch_response` frame or unparseable payload arrived.
    Protocol,
    /// The daemon refused: outside the binding, malformed, or the fetch failed.
    FetchRefused,
    OutOfMemory,
};

/// Cap on the reply frame — a path and a reason are small; this is a
/// runaway-parent guard, matching the mint channel's.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Ask the runner to fetch `repository` at `commit` into this lease's workspace,
/// blocking on the reply. Returns the workspace-relative path duped into `alloc`
/// (caller owns + frees). Every failure is typed so the caller fails closed.
///
/// On a refusal the daemon's reason is written to `reason_out` (a caller-owned
/// buffer) and `error.FetchRefused` is returned — the reason is what the model
/// reformulates against, so it must survive the error path.
pub fn request(
    ch: Channel,
    alloc: std.mem.Allocator,
    ask: PipeRequest,
    reason_out: *std.ArrayList(u8),
) FetchError![]u8 {
    const req_json = std.json.Stringify.valueAlloc(alloc, ask, .{}) catch return error.OutOfMemory;
    defer alloc.free(req_json);

    pipe_proto.writeFrame(ch.request_fd, .repo_fetch_request, req_json) catch
        return error.ChannelWrite;

    const outcome = pipe_proto.readFrame(alloc, ch.response_fd, ch.deadline_ms, MAX_RESPONSE_BYTES) catch
        return error.Protocol;
    switch (outcome) {
        .timed_out => return error.FetchTimeout,
        .eof => return error.ChannelClosed,
        .frame => |f| {
            defer alloc.free(f.payload);
            if (f.ftype != .repo_fetch_response) return error.Protocol;
            const parsed = std.json.parseFromSlice(PipeResponse, alloc, f.payload, .{}) catch
                return error.Protocol;
            defer parsed.deinit();
            if (!parsed.value.ok) {
                reason_out.appendSlice(alloc, parsed.value.reason) catch return error.OutOfMemory;
                return error.FetchRefused;
            }
            return alloc.dupe(u8, parsed.value.path) catch return error.OutOfMemory;
        },
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────
// The test plays the parent: it reads the child's ask off one pipe and writes a
// reply down the other, exercising the full synchronous round-trip in-process.

const testing = std.testing;

const Harness = struct {
    ch: Channel,
    parent_read: std.posix.fd_t,
    parent_write: std.posix.fd_t,

    fn init(deadline_ms: i64) !Harness {
        const req = try pipe_proto.testOsPipe();
        const resp = try pipe_proto.testOsPipe();
        return .{
            .ch = .{ .request_fd = req[1], .response_fd = resp[0], .deadline_ms = deadline_ms },
            .parent_read = req[0],
            .parent_write = resp[1],
        };
    }

    fn deinit(self: Harness) void {
        pipe_proto.testOsClose(self.ch.request_fd);
        pipe_proto.testOsClose(self.ch.response_fd);
        pipe_proto.testOsClose(self.parent_read);
        pipe_proto.testOsClose(self.parent_write);
    }

    fn reply(self: Harness, resp: PipeResponse) !void {
        const json = try std.json.Stringify.valueAlloc(testing.allocator, resp, .{});
        defer testing.allocator.free(json);
        try pipe_proto.writeFrame(self.parent_write, .repo_fetch_response, json);
    }
};

const SHA1 = "9a078a7b5c1d4e2f60718293a4b5c6d7e8f90123";

fn futureDeadline() i64 {
    return @import("common").clock.nowMillis() + 5_000;
}

test "a fetch round-trips a ready path and names no workspace on the wire" {
    const h = try Harness.init(futureDeadline());
    defer h.deinit();
    try h.reply(.{ .ok = true, .path = "repo" });

    var reason: std.ArrayList(u8) = .empty;
    defer reason.deinit(testing.allocator);
    const path = try request(h.ch, testing.allocator, .{ .repository = "acme/payments", .commit = SHA1 }, &reason);
    defer testing.allocator.free(path);
    try testing.expectEqualStrings("repo", path);

    // The ask carries WHAT, never WHERE: no workspace, no lease id, no absolute
    // path, so there is nothing here for a prompt-injected child to forge.
    const out = try pipe_proto.readFrame(testing.allocator, h.parent_read, h.ch.deadline_ms, 4096);
    defer testing.allocator.free(out.frame.payload);
    try testing.expectEqual(pipe_proto.FrameType.repo_fetch_request, out.frame.ftype);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, "acme/payments") != null);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, "workspace") == null);
    try testing.expect(std.mem.indexOf(u8, out.frame.payload, "lease") == null);
}

test "a refusal surfaces its reason so the model can reformulate" {
    const h = try Harness.init(futureDeadline());
    defer h.deinit();
    const refusal = "repository is outside the fleet's binding";
    try h.reply(.{ .ok = false, .reason = refusal });

    var reason: std.ArrayList(u8) = .empty;
    defer reason.deinit(testing.allocator);
    try testing.expectError(
        error.FetchRefused,
        request(h.ch, testing.allocator, .{ .repository = "otherorg/payments", .commit = SHA1 }, &reason),
    );
    // The reason survives the error path — an unexplained refusal leaves the
    // model nothing to do but retry the same ask.
    try testing.expectEqualStrings(refusal, reason.items);
}

test "a closed response channel and a skewed frame both fail closed" {
    {
        const h = try Harness.init(futureDeadline());
        pipe_proto.testOsClose(h.parent_write); // no reply, clean EOF
        defer {
            pipe_proto.testOsClose(h.ch.request_fd);
            pipe_proto.testOsClose(h.ch.response_fd);
            pipe_proto.testOsClose(h.parent_read);
        }
        var reason: std.ArrayList(u8) = .empty;
        defer reason.deinit(testing.allocator);
        try testing.expectError(
            error.ChannelClosed,
            request(h.ch, testing.allocator, .{ .repository = "acme/payments", .commit = SHA1 }, &reason),
        );
    }
    {
        const h = try Harness.init(futureDeadline());
        defer h.deinit();
        // A credential reply on the fetch channel is wire skew, not a fetch.
        try pipe_proto.writeFrame(h.parent_write, .credential_response, "{\"ok\":true}");
        var reason: std.ArrayList(u8) = .empty;
        defer reason.deinit(testing.allocator);
        try testing.expectError(
            error.Protocol,
            request(h.ch, testing.allocator, .{ .repository = "acme/payments", .commit = SHA1 }, &reason),
        );
    }
}

test "an elapsed lease deadline ends the round-trip rather than blocking the child" {
    const h = try Harness.init(@import("common").clock.nowMillis() - 1);
    defer h.deinit();
    var reason: std.ArrayList(u8) = .empty;
    defer reason.deinit(testing.allocator);
    try testing.expectError(
        error.FetchTimeout,
        request(h.ch, testing.allocator, .{ .repository = "acme/payments", .commit = SHA1 }, &reason),
    );
}
