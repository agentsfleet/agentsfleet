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
const dts = @import("deadline_test_support.zig");

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
        // An empty body names no cause, so the stop keeps the historical class.
        try testing.expectEqual(client.RenewResult{ .terminal = .{ .status = status, .reason = .renewal_terminate } }, out);
    }
}

test "classifyRenew: a 402 carrying UZ-RUN-015 is a fleet budget breach" {
    const body = "{\"error_code\":\"UZ-RUN-015\",\"detail\":\"Fleet budget exhausted\"}";
    const out = try client.classifyRenew(testing.allocator, 402, body);
    try testing.expectEqual(client.RenewResult{ .terminal = .{ .status = 402, .reason = .budget_breach } }, out);
}

test "classifyRenew: any other refusal cause stays renewal_terminate" {
    // The tenant's credit pool (UZ-RUN-012) must not be mistaken for the fleet's
    // own ceiling, and an unreadable body must never invent a specific cause.
    const cases = [_][]const u8{
        "{\"error_code\":\"UZ-RUN-012\"}", // credit exhausted, same 402
        "{\"error_code\":\"UZ-RUN-011\"}", // lease lost
        "{\"error_code\":\"\"}", // present but empty
        "{\"detail\":\"no code field\"}", // no error_code at all
        "{truncated", // unparseable
        "", // empty
    };
    for (cases) |body| {
        const out = try client.classifyRenew(testing.allocator, 402, body);
        try testing.expectEqual(client.RenewResult{ .terminal = .{ .status = 402, .reason = .renewal_terminate } }, out);
    }
}

test "classifyRenew: UZ-RUN-015 on a NON-402 terminal status is not a budget breach" {
    // Only the 402 budget refusal carries UZ-RUN-015 server-side. A 401/404/409
    // whose body happens to carry it (proxy injection, a future reuse) must NOT
    // be relabeled budget_breach — the classifier gates on status == 402.
    const body = "{\"error_code\":\"UZ-RUN-015\"}";
    inline for (.{ 401, 404, 409 }) |status| {
        const out = try client.classifyRenew(testing.allocator, status, body);
        try testing.expectEqual(client.RenewResult{ .terminal = .{ .status = status, .reason = .renewal_terminate } }, out);
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
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);
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
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);
    defer c.deinit();

    // Nobody accepts or responds: the armed call-deadline watchdog must shut
    // the socket down and bound the read (SO_RCVTIMEO was rejected — the
    // threaded Io recv path panics on its EAGAIN; see call_deadline.zig).
    const t0 = common.clock.nowMillis();
    try testing.expectError(error.RequestFailed, c.heartbeat(alloc, "agt_rtest", DEADLINE_PROBE_MS));
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
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);

    const first = try c.heartbeat(alloc, "agt_rtest", DEADLINE_PROBE_MS);
    try testing.expectEqual(.ok, first.status);
    const second = try c.heartbeat(alloc, "agt_rtest", DEADLINE_PROBE_MS);
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
        // Reviewed: `sched` is a BORROWED pointer to the runner root's one
        // process scheduler — the client neither owns nor deinits it, and it
        // holds no descriptor (a deadline names a connection generation, and
        // the socket it may shut down is published per attempt by `send`).
        const known = comptime (std.mem.eql(u8, f.name, "base_url") or std.mem.eql(u8, f.name, "io") or
            std.mem.eql(u8, f.name, "http") or std.mem.eql(u8, f.name, "host") or
            std.mem.eql(u8, f.name, "port") or std.mem.eql(u8, f.name, "tls") or
            std.mem.eql(u8, f.name, "sched"));
        if (!known)
            @compileError("control-plane client gained field '" ++ f.name ++ "' — review for fd/credential ownership before it lands");
    }
}

const RENEW_OK_BODY = "{\"lease_expires_at\":1900000000123}";

/// One-shot renew responder: accepts ONE connection, reads ONE request
/// (headers + Content-Length body), captures the body bytes, replies 200 with
/// a RenewResponse — so the test below asserts the PRODUCTION client put the
/// cumulative splits on the wire (an empty-body regression fails here).
const RenewBodyStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    body_buf: [512]u8 = [_]u8{0} ** 512,
    body_len: usize = 0,

    fn run(self: *RenewBodyStub) void {
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
            const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch return;
            if (n == 0) break;
            total += n;
        }
        const body = rbuf[header_end..@min(total, header_end + content_len)];
        @memcpy(self.body_buf[0..body.len], body);
        self.body_len = body.len;
        var wbuf: [256]u8 = undefined;
        var w = conn.writer(self.io, &wbuf);
        w.interface.print(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
            .{ RENEW_OK_BODY.len, RENEW_OK_BODY },
        ) catch return;
        w.interface.flush() catch return;
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

test "renew puts the cumulative splits on the wire as the POST body (production client)" {
    const alloc = testing.allocator;
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var stub = RenewBodyStub{ .io = io, .listener = &listener };
    const responder = std.Thread.spawn(.{}, RenewBodyStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);
    defer c.deinit();

    const out = try c.renew(alloc, "agt_rtest", "lease-1", .{ .input_tokens = 100, .cached_input_tokens = 0, .output_tokens = 40 }, DEADLINE_PROBE_MS);
    responder.join();

    try testing.expectEqual(client.RenewResult{ .renewed = 1_900_000_000_123 }, out);
    // The captured wire bytes parse back to exactly the request struct — the
    // empty-body under-billing regression this milestone fixes dies here.
    try testing.expect(stub.body_len > 0);
    const parsed = try std.json.parseFromSlice(@import("contract").protocol.RenewRequest, alloc, stub.body_buf[0..stub.body_len], .{});
    defer parsed.deinit();
    try testing.expectEqual(@as(u32, 100), parsed.value.input_tokens);
    try testing.expectEqual(@as(u32, 0), parsed.value.cached_input_tokens);
    try testing.expectEqual(@as(u32, 40), parsed.value.output_tokens);
}

// §4 / Dimension 4.1 — post()/get() build an Allocating response writer
// (`aw`) and only hand its buffer off via `toOwnedSlice()` on the SUCCESS path. A
// fetch failure after the server has already streamed partial bytes leaves those
// bytes owned by `aw`; without the errdefer this milestone adds, that partial
// buffer leaks.
//
// The fault: a `Transfer-Encoding: chunked` response whose first data chunk is
// delivered in full (so the client decodes it INTO `aw`) but whose stream is then
// cut before the terminating 0-length chunk. Unlike a Content-Length body — where
// the client tolerates a short read at EOF and returns the partial body as a bogus
// success — a chunked stream that ends without its terminator is an unambiguous
// framing error, so fetch fails with bytes already in `aw`. std.testing.allocator
// turns any leaked `aw` buffer into a hard failure, so a green run IS the zero-leak
// proof for the errdefer, on both verbs.
const CHUNK_SIZE_HEX = "1000"; // 0x1000 = 4096 bytes — one complete data chunk
const CHUNK_DATA = "x" ** 4096; // decoded into `aw` before the framing error hits

const ChunkedThenCutStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,

    fn run(self: *ChunkedThenCutStub) void {
        const conn = self.listener.accept(self.io) catch return;
        defer conn.close(self.io); // return → stream ends with no terminating 0-chunk
        var rbuf: [2048]u8 = undefined;
        var total: usize = 0;
        while (std.mem.indexOf(u8, rbuf[0..total], "\r\n\r\n") == null) {
            const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch return;
            if (n == 0) return;
            total += n;
            if (total == rbuf.len) return;
        }
        var wbuf: [8192]u8 = undefined;
        var w = conn.writer(self.io, &wbuf);
        // Headers + exactly one complete chunk, then close — the required
        // `0\r\n\r\n` terminator never arrives, so the decoder errors mid-stream.
        w.interface.print(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{s}\r\n{s}\r\n",
            .{ CHUNK_SIZE_HEX, CHUNK_DATA },
        ) catch return;
        w.interface.flush() catch return;
    }
};

fn expectVerbReleasesBufferOnMidStreamFailure(comptime verb: enum { post, get }) !void {
    const alloc = testing.allocator;
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var stub = ChunkedThenCutStub{ .io = io, .listener = &listener };
    const responder = std.Thread.spawn(.{}, ChunkedThenCutStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);
    defer c.deinit();

    const result = switch (verb) {
        .post => c.post(alloc, "/v1/runners/me/heartbeats", "agt_rtest", "", DEADLINE_PROBE_MS),
        .get => c.get(alloc, "/v1/runners/me", "agt_rtest", DEADLINE_PROBE_MS),
    };
    responder.join();

    // The verb must surface the mid-stream failure as an error, never a bogus 200.
    if (result) |ok| {
        alloc.free(ok.body); // unreachable on the fix's contract; free to stay leak-clean
        return error.MidStreamFailureNotSurfaced;
    } else |err| {
        try testing.expect(err == client.ClientError.RequestFailed);
    }
    // testing.allocator asserts zero leaks at test end — the errdefer's proof.
}

test "test_post_get_release_buffer_on_mid_stream_fetch_failure" {
    try expectVerbReleasesBufferOnMidStreamFailure(.post);
    try expectVerbReleasesBufferOnMidStreamFailure(.get);
}

test "checkStatus: 401/403 map to Unauthorized, other non-2xx to BadStatus, 2xx passes" {
    // The fail-loud contract: a rejected token (401/403) is a PERMANENT reject,
    // kept distinct from a transient non-2xx so the control loop exits
    // token_rejected instead of retrying a stale token forever as transport loss
    // (the invisible crash-loop this fixes). The status codes are the contract.
    inline for (.{ 401, 403 }) |status| {
        try testing.expectError(client.ClientError.Unauthorized, client.checkStatus(status));
    }
    inline for (.{ 400, 404, 409, 429, 500, 502, 503 }) |status| {
        try testing.expectError(client.ClientError.BadStatus, client.checkStatus(status));
    }
    inline for (.{ 200, 201, 202, 204 }) |status| {
        try client.checkStatus(status); // 2xx → no error
    }
}
