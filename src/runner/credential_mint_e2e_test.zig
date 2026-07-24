//! End-to-end proofs for the runner half of the on-demand credential-mint channel
//! (M102 §3, Dimensions 3.1 + 3.3) — the child→runner→daemon round-trip, with the
//! daemon a real loopback HTTP stub so the test crosses the actual control-plane
//! client (`cp.mint`) without the daemon's pg/httpz graph (runner isolation).
//!
//! The full path is two legs meeting at the `/v1/runners/me/credentials/mint`
//! wire: this file owns the RUNNER leg (real child pipe frames → real parent read
//! loop → real `std.http` client → stub daemon); the daemon leg (real endpoint +
//! DB lease + broker, Invariant 2) is `credentials_mint_integration_test.zig` in
//! the agentsfleetd suite. Both agree on the wire via `protocol_credentials.zig`.
//!
//!   3.1  test_child_requests_token_via_runner — the parent services a child's
//!        `credential_request` by forwarding to the (stub) daemon and frames the
//!        minted token back down the child's stdin.
//!   3.3  test_on_demand_mint_no_trigger — a mintable `${secrets.github.token}`
//!        placeholder at the tool boundary provokes a fresh mint with NO inbound
//!        event/webhook (the "steer after idle" product moment).
//!   + the `cp.mint` verb's fail-closed contract (non-2xx → rejected), which had
//!     no direct test.
//!
//! Cross-platform (in-process pipes + a loopback TCP stub, no fork); the real
//! forked-child process mechanics live in `sandbox_integration_test.zig`. Skips
//! only when a loopback socket cannot bind.

const std = @import("std");
const common = @import("common");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");
const credential_request = @import("engine/credential_request.zig");
const child_supervisor = @import("child_supervisor.zig");
const secret_substitution = @import("engine/runtime/secret_substitution.zig");
const cp = @import("daemon/control_plane_client.zig");
const dts = @import("daemon/deadline_test_support.zig");

const ALLOC = std.testing.allocator;
const ActivityFrame = contract.activity.ActivityFrame;

const MINTED_TOKEN = "ghs_e2e_minted";
// Mirrors `protocol.MintCredentialResponse{ token, expires_at_ms }`.
const MINT_OK_BODY = "{\"token\":\"" ++ MINTED_TOKEN ++ "\",\"expires_at_ms\":1900000000999}";
const RUNNER_TOKEN = "agt_rtest";
// cp's per-call watchdog bound — generous; the stub answers in ~ms.
const MINT_DEADLINE_MS: u31 = 2_000;
// MintStub request-capture buffer cap — a mint request is well under 1 KiB.
const MINT_STUB_BODY_CAP: usize = 1024;

// No-op child→parent sinks: no activity/memory frames flow in these tests, so the
// forwards are never called (the opaque ctx is never dereferenced).
fn discardActivity(_: *anyopaque, _: ActivityFrame) void {}
fn discardMemory(_: *anyopaque, _: []const u8) void {}

/// Loopback daemon stub for the mint endpoint — mirrors `control_plane_client_test`'s
/// `RenewBodyStub`: accepts ONE connection, reads one request (headers +
/// Content-Length body), captures the body for assertions, replies with `status`
/// (200 → the canned token; anything else → the fail-closed path).
const MintStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    status: u16,
    body_buf: [MINT_STUB_BODY_CAP]u8 = [_]u8{0} ** MINT_STUB_BODY_CAP,
    body_len: usize = 0,

    fn run(self: *MintStub) void {
        const conn = self.listener.accept(self.io) catch return;
        defer conn.close(self.io);
        var rbuf: [4096]u8 = undefined;
        var total: usize = 0;
        var header_end: usize = 0;
        while (true) {
            if (std.mem.indexOf(u8, rbuf[0..total], "\r\n\r\n")) |idx| {
                header_end = idx + 4;
                break;
            }
            const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch return;
            if (n == 0) return;
            total += n;
            if (total == rbuf.len) return;
        }
        const content_len = parseContentLength(rbuf[0..header_end]) orelse 0;
        while (total < header_end + content_len) {
            const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch break;
            if (n == 0) break;
            total += n;
        }
        const captured = rbuf[header_end..@min(total, header_end + content_len)];
        @memcpy(self.body_buf[0..captured.len], captured);
        self.body_len = captured.len;

        var wbuf: [256]u8 = undefined;
        var w = conn.writer(self.io, &wbuf);
        if (self.status == 200) {
            w.interface.print(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
                .{ MINT_OK_BODY.len, MINT_OK_BODY },
            ) catch return;
        } else {
            const err_body = "{\"code\":\"UZ-GH-002\"}";
            w.interface.print(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
                .{ err_body.len, err_body },
            ) catch return;
        }
        w.interface.flush() catch return;
    }

    fn body(self: *const MintStub) []const u8 {
        return self.body_buf[0..self.body_len];
    }
};

fn parseContentLength(headers: []const u8) ?usize {
    var it = std.mem.splitSequence(u8, headers, "\r\n");
    while (it.next()) |line| {
        const prefix = "content-length:";
        if (line.len > prefix.len and std.ascii.startsWithIgnoreCase(line, prefix)) {
            const v = std.mem.trim(u8, line[prefix.len..], " ");
            return std.fmt.parseInt(usize, v, 10) catch null;
        }
    }
    return null;
}

/// Local port of a bound listener (the test binds port 0). Mirrors the
/// control-plane client test's helper.
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the != 0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// The cp-backed mint hook — the production `MintForwarder` shape (lease_run.zig),
/// inlined: `onMint` forwards the child's ask to the daemon over the real control
/// plane, binding to the lease's workspace server-side (the child names no workspace).
const Forwarder = struct {
    client: *cp,
    lease_id: []const u8,

    fn onMint(ctx: *anyopaque, alloc: std.mem.Allocator, int: []const u8, scope: ?[]const u8) child_supervisor.CredentialOutcome {
        const self: *Forwarder = @ptrCast(@alignCast(ctx));
        return switch (self.client.mint(alloc, RUNNER_TOKEN, self.lease_id, int, scope, MINT_DEADLINE_MS)) {
            .minted => |m| .{ .minted = .{ .token = m.token, .expires_at_ms = m.expires_at_ms } },
            .rejected => .rejected,
        };
    }
    fn hook(self: *Forwarder) child_supervisor.MintHook {
        return .{ .ctx = self, .onMint = onMint };
    }
};

test "test_child_requests_token_via_runner" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = MintStub{ .io = io, .listener = &listener, .status = 200 };
    const responder = std.Thread.spawn(.{}, MintStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var client = cp.init(ALLOC, io, try deadlines.start(ALLOC), url);
    defer client.deinit();
    var forwarder = Forwarder{ .client = &client, .lease_id = "lease-e2e-3-1" };

    // The child's stdout (parent reads out[0]) + stdin (the test reads resp[0]).
    const out = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(out[0]);
    const resp = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(resp[0]);

    // The exact frames the child writes: a credential_request, then a terminal
    // result. (The child's own `mint` send/decode is unit-proven in
    // credential_request.zig; the system under test here is the parent's forward.)
    try pipe_proto.writeFrame(out[1], .credential_request, "{\"integration\":\"github\"}");
    try pipe_proto.writeFrame(out[1], .result, "{\"exit_ok\":true}");
    pipe_proto.testOsClose(out[1]);

    var dummy: u8 = 0;
    const sink = child_supervisor.ActivitySink{ .ctx = &dummy, .forward = discardActivity };
    const mem_sink = child_supervisor.MemorySink{ .ctx = &dummy, .forward = discardMemory };
    const dl = common.clock.nowMillis() + 5_000;
    const outcome = try child_supervisor.readResult(ALLOC, out[0], resp[1], dl, sink, mem_sink, null, forwarder.hook());
    defer ALLOC.free(outcome.bytes);
    responder.join();

    // The runner forwarded to the (stub) daemon over the real control plane and
    // framed the minted token back on the child's stdin.
    pipe_proto.testOsClose(resp[1]);
    const reply = try pipe_proto.readFrame(ALLOC, resp[0], dl, 4096);
    defer ALLOC.free(reply.frame.payload);
    try std.testing.expectEqual(pipe_proto.FrameType.credential_response, reply.frame.ftype);
    const parsed = try std.json.parseFromSlice(credential_request.PipeResponse, ALLOC, reply.frame.payload, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value.ok);
    try std.testing.expectEqualStrings(MINTED_TOKEN, parsed.value.token);

    // Invariant 2 on the wire: the forward named the lease + integration, NEVER a
    // workspace — there is nothing for a prompt-injected child to forge.
    try std.testing.expect(std.mem.indexOf(u8, stub.body(), "lease-e2e-3-1") != null);
    try std.testing.expect(std.mem.indexOf(u8, stub.body(), "github") != null);
    try std.testing.expect(std.mem.indexOf(u8, stub.body(), "workspace") == null);
}

/// The parent read loop, run on a thread so the child's blocking `substitute`
/// round-trip is serviced live (it cannot be pre-buffered — the token comes from
/// the real cp→stub forward, not the test).
const Parent = struct {
    fn run(reader_fd: std.posix.fd_t, writer_fd: std.posix.fd_t, fwd: *Forwarder) void {
        var dummy: u8 = 0;
        const sink = child_supervisor.ActivitySink{ .ctx = &dummy, .forward = discardActivity };
        const mem_sink = child_supervisor.MemorySink{ .ctx = &dummy, .forward = discardMemory };
        const dl = common.clock.nowMillis() + 5_000;
        const outcome = child_supervisor.readResult(ALLOC, reader_fd, writer_fd, dl, sink, mem_sink, null, fwd.hook()) catch return;
        ALLOC.free(outcome.bytes); // result-frame payload (owned); empty + safe on EOF/terminate
    }
};

test "test_on_demand_mint_no_trigger" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = MintStub{ .io = io, .listener = &listener, .status = 200 };
    const responder = std.Thread.spawn(.{}, MintStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var client = cp.init(ALLOC, io, try deadlines.start(ALLOC), url);
    defer client.deinit();
    var forwarder = Forwarder{ .client = &client, .lease_id = "lease-e2e-3-3" };

    // Child writes requests on req[1], reads replies on resp[0]; parent reads
    // req[0], writes replies on resp[1].
    const req = try pipe_proto.testOsPipe();
    const resp = try pipe_proto.testOsPipe();
    defer for ([_]std.posix.fd_t{ req[0], req[1], resp[0], resp[1] }) |fd| pipe_proto.testOsClose(fd);

    const channel = credential_request.Channel{
        .request_fd = req[1],
        .response_fd = resp[0],
        .deadline_ms = common.clock.nowMillis() + 5_000,
    };
    const parent_thread = try std.Thread.spawn(.{}, Parent.run, .{ req[0], resp[1], &forwarder });

    // The tool boundary: a mintable `${secrets.github.token}` placeholder. NO event
    // or webhook woke this session — the substitution alone provokes a fresh mint
    // at the moment the tool needs the token.
    var arena_state = std.heap.ArenaAllocator.init(ALLOC);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    var resolver = credential_request.MintResolver{
        .mintable = &.{.{ .name = "github", .integration = "github" }},
        .channel = channel,
    };
    const out = try secret_substitution.substitute(arena, "Authorization: Bearer ${secrets.github.token}", null, &resolver);

    // End the parent loop: a terminal result frame on the child's stdout.
    try pipe_proto.writeFrame(req[1], .result, "{\"exit_ok\":true}");
    parent_thread.join();
    responder.join();

    try std.testing.expectEqualStrings("Authorization: Bearer " ++ MINTED_TOKEN, out);
    try std.testing.expect(std.mem.indexOf(u8, stub.body(), "github") != null);
    try std.testing.expect(std.mem.indexOf(u8, stub.body(), "workspace") == null);
}

test "cp mint forwards lease_id only and fails closed on a non-2xx (§3)" {
    const io = common.globalIo();

    // 200 → minted; the wire carries lease_id + integration, never a workspace.
    {
        var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
        var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
        defer listener.deinit(io);
        const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
        var stub = MintStub{ .io = io, .listener = &listener, .status = 200 };
        const responder = std.Thread.spawn(.{}, MintStub.run, .{&stub}) catch return error.SkipZigTest;

        var url_buf: [48]u8 = undefined;
        const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var client = cp.init(ALLOC, io, try deadlines.start(ALLOC), url);
        defer client.deinit();

        const outcome = client.mint(ALLOC, RUNNER_TOKEN, "lease-ok", "github", null, MINT_DEADLINE_MS);
        responder.join();
        try std.testing.expect(outcome == .minted);
        try std.testing.expectEqualStrings(MINTED_TOKEN, outcome.minted.token);
        ALLOC.free(outcome.minted.token);
        try std.testing.expect(std.mem.indexOf(u8, stub.body(), "lease-ok") != null);
        try std.testing.expect(std.mem.indexOf(u8, stub.body(), "github") != null);
        try std.testing.expect(std.mem.indexOf(u8, stub.body(), "workspace") == null);
    }

    // Non-2xx (a typed UZ-GH-* envelope) → rejected, no token: the child aborts
    // its tool call rather than dispatch with a blank/stale credential.
    {
        var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
        var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
        defer listener.deinit(io);
        const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
        var stub = MintStub{ .io = io, .listener = &listener, .status = 502 };
        const responder = std.Thread.spawn(.{}, MintStub.run, .{&stub}) catch return error.SkipZigTest;

        var url_buf: [48]u8 = undefined;
        const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var client = cp.init(ALLOC, io, try deadlines.start(ALLOC), url);
        defer client.deinit();

        const outcome = client.mint(ALLOC, RUNNER_TOKEN, "lease-fail", "github", null, MINT_DEADLINE_MS);
        responder.join();
        try std.testing.expect(outcome == .rejected);
    }
}
