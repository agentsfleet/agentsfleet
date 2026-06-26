//! credential_request.zig — the child→runner on-demand mint channel (M102 §3).
//!
//! The sandboxed child holds no token, no control-plane URL, and no datastore
//! credential (RULE VLT) — so when the tool bridge resolves a *mintable*
//! placeholder (`${secrets.github.token}`), it cannot mint itself. It asks its
//! runner: a `credential_request` frame up the stdout pipe, then it blocks
//! reading the `credential_response` frame back down the stdin pipe. The parent
//! supervisor services the ask inline (forwards to the daemon broker over the
//! agt_r plane, `data_flow.md` §B) and frames the short-lived token back.
//!
//! This module owns the child half: the two pipe payload shapes (shared verbatim
//! with the parent reader, RULE UFS) and the synchronous `mint` round-trip. The
//! channel rides the EXISTING stdin/stdout pipes (the memory channel's pattern) —
//! no new descriptor, no new sandbox hole. The round-trip is bounded by the lease
//! deadline so a wedged parent can never block the child past `lease_expires_at`.
//!
//! Fail closed: any transport loss, protocol skew, or typed rejection surfaces as
//! an error, and the tool bridge aborts the call rather than dispatching with a
//! blank or stale credential.

const std = @import("std");
const pipe_proto = @import("../pipe_proto.zig");

/// child→parent mint ask (`credential_request` frame payload). The child names
/// only the integration (+ an optional scope narrowing) — never a workspace or a
/// lease id: the parent binds the mint to the lease's workspace server-side
/// (Invariant 2), so there is nothing here for a prompt-injected child to forge.
pub const PipeRequest = struct {
    integration: []const u8,
    scope: ?[]const u8 = null,
};

/// parent→child mint reply (`credential_response` frame payload). `ok` gates the
/// token: a rejection (unknown integration / reconnect-required / mint-failed)
/// rides as `ok=false` with an empty token, and the child fails the call closed.
/// `token` is secret (VLT) — it lives only in the child's per-call arena, is
/// substituted at dispatch, and never enters fleet context or any activity frame.
pub const PipeResponse = struct {
    ok: bool,
    token: []const u8 = "",
    expires_at_ms: i64 = 0,
};

/// The child's two pipe ends + the lease wall-clock bound. `request_fd` is the
/// child's stdout (it writes the ask there, multiplexed with activity frames);
/// `response_fd` is the child's stdin (it reads exactly one reply there). The
/// child is single-threaded during a turn, so the request is the only frame in
/// flight while it blocks for the reply — no interleave with activity frames.
pub const Channel = struct {
    request_fd: std.posix.fd_t,
    response_fd: std.posix.fd_t,
    /// Absolute epoch-ms deadline (the lease's `lease_expires_at`).
    deadline_ms: i64,
};

pub const MintError = error{
    /// Could not write the request frame (parent closed stdout-read end).
    ChannelWrite,
    /// Parent closed the response pipe at a frame boundary before replying.
    ChannelClosed,
    /// The lease deadline elapsed mid round-trip.
    MintTimeout,
    /// A non-`credential_response` frame or unparseable payload arrived — wire skew.
    Protocol,
    /// The broker refused (unknown integration / reconnect-required / mint-failed).
    MintRejected,
    OutOfMemory,
};

/// Cap on the reply frame — a token is small; this is a runaway-parent guard.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Ask the runner to mint a short-lived credential for `integration`, blocking on
/// the reply. Returns the token duped into `alloc` (caller owns + frees — the tool
/// bridge's per-call arena); every failure is typed so the caller fails closed.
pub fn mint(
    ch: Channel,
    alloc: std.mem.Allocator,
    integration: []const u8,
    scope: ?[]const u8,
) MintError![]u8 {
    const req_json = std.json.Stringify.valueAlloc(alloc, PipeRequest{
        .integration = integration,
        .scope = scope,
    }, .{}) catch return error.OutOfMemory;
    defer alloc.free(req_json);

    pipe_proto.writeFrame(ch.request_fd, .credential_request, req_json) catch
        return error.ChannelWrite;

    const outcome = pipe_proto.readFrame(alloc, ch.response_fd, ch.deadline_ms, MAX_RESPONSE_BYTES) catch
        return error.Protocol;
    switch (outcome) {
        .timed_out => return error.MintTimeout,
        .eof => return error.ChannelClosed,
        .frame => |f| {
            defer alloc.free(f.payload);
            if (f.ftype != .credential_response) return error.Protocol;
            const parsed = std.json.parseFromSlice(PipeResponse, alloc, f.payload, .{}) catch
                return error.Protocol;
            defer parsed.deinit();
            if (!parsed.value.ok) return error.MintRejected;
            return alloc.dupe(u8, parsed.value.token) catch return error.OutOfMemory;
        },
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────
// The test plays the parent: it reads the child's request off one pipe and writes
// a reply down the other, exercising the full synchronous round-trip in-process.

const testing = std.testing;

/// Build a child `Channel` over two fresh os pipes + return the parent's ends.
const Harness = struct {
    ch: Channel,
    parent_read: std.posix.fd_t, // parent reads the child's request here
    parent_write: std.posix.fd_t, // parent writes the child's response here

    fn init(deadline_ms: i64) !Harness {
        const req = try pipe_proto.testOsPipe(); // [read, write]; child writes [1]
        const resp = try pipe_proto.testOsPipe(); // child reads [0]
        return .{
            .ch = .{ .request_fd = req[1], .response_fd = resp[0], .deadline_ms = deadline_ms },
            .parent_read = req[0],
            .parent_write = resp[1],
        };
    }

    /// Read the child's request frame (caller owns the returned payload).
    fn readRequest(self: Harness) !pipe_proto.Frame {
        const out = try pipe_proto.readFrame(testing.allocator, self.parent_read, self.ch.deadline_ms, 4096);
        return out.frame;
    }

    fn deinit(self: Harness) void {
        pipe_proto.testOsClose(self.ch.request_fd);
        pipe_proto.testOsClose(self.ch.response_fd);
        pipe_proto.testOsClose(self.parent_read);
        pipe_proto.testOsClose(self.parent_write);
    }
};

test "mint round-trips an ok response into the token" {
    const clock = @import("common").clock;
    const h = try Harness.init(clock.nowMillis() + 5_000);
    defer h.deinit();

    // The round-trip is synchronous, so the test (the "parent") pre-buffers the
    // reply before calling `mint` — the pipe holds it until the child reads. The
    // child's request lands in the other pipe's buffer for the post-hoc assert.
    const reply = try std.json.Stringify.valueAlloc(testing.allocator, PipeResponse{ .ok = true, .token = "ghs_live", .expires_at_ms = 999 }, .{});
    defer testing.allocator.free(reply);
    try pipe_proto.writeFrame(h.parent_write, .credential_response, reply);

    const token = try mint(h.ch, testing.allocator, "github", null);
    defer testing.allocator.free(token);
    try testing.expectEqualStrings("ghs_live", token);

    // The child wrote exactly the integration it was asked to mint for.
    const req = try h.readRequest();
    defer testing.allocator.free(req.payload);
    try testing.expectEqual(pipe_proto.FrameType.credential_request, req.ftype);
    try testing.expect(std.mem.indexOf(u8, req.payload, "github") != null);
}

test "mint fails closed on a rejection reply" {
    const clock = @import("common").clock;
    const h = try Harness.init(clock.nowMillis() + 5_000);
    defer h.deinit();
    const reply = try std.json.Stringify.valueAlloc(testing.allocator, PipeResponse{ .ok = false }, .{});
    defer testing.allocator.free(reply);
    try pipe_proto.writeFrame(h.parent_write, .credential_response, reply);
    try testing.expectError(error.MintRejected, mint(h.ch, testing.allocator, "github", null));
}

test "mint surfaces a closed response channel" {
    const clock = @import("common").clock;
    const h = try Harness.init(clock.nowMillis() + 5_000);
    // Close the parent's write end with no reply → clean EOF for the child.
    pipe_proto.testOsClose(h.parent_write);
    defer {
        pipe_proto.testOsClose(h.ch.request_fd);
        pipe_proto.testOsClose(h.ch.response_fd);
        pipe_proto.testOsClose(h.parent_read);
    }
    try testing.expectError(error.ChannelClosed, mint(h.ch, testing.allocator, "github", null));
}

test "mint rejects a wrong-type reply frame as protocol skew" {
    const clock = @import("common").clock;
    const h = try Harness.init(clock.nowMillis() + 5_000);
    defer h.deinit();
    try pipe_proto.writeFrame(h.parent_write, .activity, "{}"); // not a credential_response
    try testing.expectError(error.Protocol, mint(h.ch, testing.allocator, "github", null));
}
