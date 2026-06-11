//! Unit tests for the control-plane client's `/renew` status mapping: the pure
//! `classifyRenew` (HTTP status + body → RenewResult) and the
//! `isTerminalRenewStatus` classifier. No HTTP — the (status, body) pairs stand
//! in for server responses, so the fail-safe contract (2xx renews, a definitive
//! 4xx terminates, every other status retries) is asserted directly.
//!
//! pin test: the HTTP status codes are the contract this maps, kept as literals.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const client = @import("control_plane_client.zig");

test "classifyRenew: a 2xx parses the new kill deadline into renewed" {
    const out = try client.classifyRenew(testing.allocator, 200, "{\"lease_expires_at\":1900000000123}");
    try testing.expectEqual(client.RenewResult{ .renewed = 1_900_000_000_123 }, out);
}

test "classifyRenew: a 2xx with an unparseable body is a malformed response" {
    try testing.expectError(error.MalformedResponse, client.classifyRenew(testing.allocator, 200, "{not json"));
}

test "classifyRenew: each terminal 4xx maps to terminal carrying that status" {
    inline for (.{ 401, 402, 404, 409 }) |status| {
        const out = try client.classifyRenew(testing.allocator, status, "");
        try testing.expectEqual(client.RenewResult{ .terminal = status }, out);
    }
}

test "classifyRenew: non-terminal 4xx and all 5xx are retryable BadStatus" {
    inline for (.{ 400, 403, 408, 429, 500, 503 }) |status| {
        try testing.expectError(error.BadStatus, client.classifyRenew(testing.allocator, status, ""));
    }
}

test "isTerminalRenewStatus: only 401/402/404/409 are terminal" {
    inline for (.{ 401, 402, 404, 409 }) |s| try testing.expect(client.isTerminalRenewStatus(s));
    inline for (.{ 200, 400, 403, 408, 410, 429, 500, 503 }) |s| try testing.expect(!client.isTerminalRenewStatus(s));
}

test "the persistent control-plane socket cannot cross exec (CLOEXEC)" {
    // The client now holds a persistent keep-alive connection pool, so the old
    // "no persistent fd" pin upgrades to the property that actually protects
    // the forked child: the threaded Io opens sockets with SOCK_CLOEXEC, so
    // the credential-bearing socket can never survive the exec into the
    // sandboxed agent (bwrap additionally closes unpassed fds in isolated
    // tiers). This pins it on a live pooled connection.
    const alloc = testing.allocator;
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var c = client.init(alloc, io, url);
    defer c.deinit();

    const host = try std.Io.net.HostName.init("127.0.0.1");
    const conn = c.http.connect(host, port, .plain) catch return error.SkipZigTest;
    defer c.http.connection_pool.release(conn, io);
    const handle = conn.stream_writer.stream.socket.handle;
    const fd_flags = std.c.fcntl(handle, std.c.F.GETFD);
    try testing.expect(fd_flags >= 0);
    try testing.expect(@as(u32, @intCast(fd_flags)) & std.posix.FD_CLOEXEC != 0);
}

/// Local port of a bound listener socket (the test binds port 0). Mirrors the
/// worker-pool integration test's helper.
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the !=0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

const DEADLINE_PROBE_MS: u31 = 500;
/// Generous ceiling: the probe must come back in ~DEADLINE_PROBE_MS; anything
/// under this proves the call is bounded (the pre-fix behaviour blocked until
/// TCP gave up — minutes to hours).
const DEADLINE_PROBE_BOUND_MS: i64 = 5_000;

test "a hung control plane surfaces a transport error within the armed deadline" {
    const alloc = testing.allocator;
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var c = client.init(alloc, io, url);
    defer c.deinit();

    // Nobody accepts or responds: the armed call-deadline watchdog must shut
    // the socket down and bound the read (SO_RCVTIMEO was rejected — the
    // threaded Io recv path panics on its EAGAIN; see call_deadline.zig).
    const t0 = common.clock.nowMillis();
    try testing.expectError(error.RequestFailed, c.heartbeat(alloc, "zrn_test", DEADLINE_PROBE_MS));
    const elapsed = common.clock.nowMillis() - t0;
    try testing.expect(elapsed < DEADLINE_PROBE_BOUND_MS);
}

const HEARTBEAT_OK_BODY = "{\"status\":\"ok\"}";

/// Keep-alive responder: accepts ONE connection and answers every request on
/// it, so the accept counter is the connection-reuse proof.
const KeepAliveStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    accepts: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),

    fn run(self: *KeepAliveStub) void {
        const conn = self.listener.accept(self.io) catch return;
        // safe because: independent statistic read after join; no ordering needed.
        _ = self.accepts.fetchAdd(1, .monotonic);
        defer conn.close(self.io);
        var rbuf: [2048]u8 = undefined;
        while (true) {
            var total: usize = 0;
            while (std.mem.indexOf(u8, rbuf[0..total], "\r\n\r\n") == null) {
                const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch return;
                if (n == 0) return; // client closed the pooled connection — done
                total += n;
                if (total == rbuf.len) return;
            }
            var wbuf: [256]u8 = undefined;
            var w = conn.writer(self.io, &wbuf);
            w.interface.print(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\n\r\n{s}",
                .{ HEARTBEAT_OK_BODY.len, HEARTBEAT_OK_BODY },
            ) catch return;
            w.interface.flush() catch return;
        }
    }
};

test "two verbs ride one pooled connection (keep-alive reuse)" {
    const alloc = testing.allocator;
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var stub = KeepAliveStub{ .io = io, .listener = &listener };
    const responder = std.Thread.spawn(.{}, KeepAliveStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var c = client.init(alloc, io, url);

    const first = try c.heartbeat(alloc, "zrn_test", DEADLINE_PROBE_MS);
    try testing.expectEqual(.ok, first.status);
    const second = try c.heartbeat(alloc, "zrn_test", DEADLINE_PROBE_MS);
    try testing.expectEqual(.ok, second.status);

    // Closing the client closes the pooled connection; the responder sees
    // read()==0 and exits, so the join cannot hang.
    c.deinit();
    responder.join();

    try testing.expectEqual(@as(u32, 1), stub.accepts.load(.monotonic));
}

test "the control-plane client's field surface is reviewed" {
    // Field allowlist tripwire: a NEW field must be reviewed for fd/credential
    // ownership before it lands. This is that review's record for the
    // persistent pool fields (http/host/port/tls) — the CLOEXEC pin above is
    // the property that makes the pool safe to hold across forks.
    const fields = @typeInfo(client).@"struct".fields;
    try testing.expectEqual(@as(usize, 7), fields.len);
    inline for (fields) |f| {
        // Guards field NAMES, not TYPES: a type change to an existing field
        // keeps the name + count and passes silently — review those by hand.
        // Reviewed: `watchdog` owns no fd (its handle copy is only valid while
        // a call is armed) and its thread is joined by client deinit.
        const known = comptime (std.mem.eql(u8, f.name, "base_url") or std.mem.eql(u8, f.name, "io") or
            std.mem.eql(u8, f.name, "http") or std.mem.eql(u8, f.name, "host") or
            std.mem.eql(u8, f.name, "port") or std.mem.eql(u8, f.name, "tls") or
            std.mem.eql(u8, f.name, "watchdog"));
        if (!known)
            @compileError("control-plane client gained field '" ++ f.name ++ "' — review for fd/credential ownership before it lands");
    }
}
