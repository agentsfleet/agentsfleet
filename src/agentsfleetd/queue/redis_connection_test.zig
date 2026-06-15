//! Slice-1 tests for `Connection.init(role)` + `Connection.command`. Uses a
//! minimal in-process RESP fake (one accept, one "+PONG\r\n" reply, park
//! until shutdown) so tests don't depend on a live Redis. Sister tests at
//! the Pool level live in `redis_pool_test.zig`.

const std = @import("std");
const common = @import("common");
const clock = @import("common").clock;
const net = std.Io.net;
const test_port = @import("../http/test_port.zig");
const Connection = @import("redis_connection.zig");
const redis_config = @import("redis_config.zig");
const log_sinks = @import("log").sinks;

// Loopback host shared across every in-process fake server below.
// `parseIp4` (for `listen`) and `alloc.dupe` (for `Config.host`) both want
// the same literal — extracting prevents drift.
const LOOPBACK_IPV4 = "127.0.0.1";

// Shutdown-loop poll tick for new fake servers introduced in this file.
// Pre-existing `PongOnce` keeps its 5ms tick to avoid opportunistic edits;
// new fakes use the canonical 10ms (matches `redis_subscriber_test.zig`).
const FAKE_SHUTDOWN_POLL_NS = 10 * std.time.ns_per_ms;

// Upper bound for `SO_RCVTIMEO`-fired timing assertions — catches
// retry-loop or busy-spin bugs that would let an "expected ~200ms"
// test silently hang for minutes. The floor is per-test (a function
// of the test's configured budget, e.g. half of 200ms).
const RCVTIMEO_CEILING_NS = 5 * std.time.ns_per_s;

// Read whatever the client sent (one underlying read), returning the count of
// buffered request bytes. Zig 0.16 `net.Stream` exposes no direct `read`; go
// through a `Stream.Reader` and `fillMore` (one read syscall; EOF on peer
// close). Returns 0 on EOF/error so callers gate the reply on `n > 0` exactly
// as the pre-0.16 `stream.read(&buf) catch 0` did.
fn fakeReadOnce(io: std.Io, stream: net.Stream) usize {
    var rbuf: [256]u8 = undefined;
    var r = stream.reader(io, &rbuf);
    r.interface.fillMore() catch return 0;
    return r.interface.bufferedLen();
}

// Write `bytes` to the accepted stream. Zig 0.16 `net.Stream` exposes no direct
// `writeAll`; go through a `Stream.Writer` and flush. Best-effort, matching the
// pre-0.16 `stream.writeAll(...) catch {}`.
fn fakeWriteAll(io: std.Io, stream: net.Stream, bytes: []const u8) void {
    var wbuf: [256]u8 = undefined;
    var w = stream.writer(io, &wbuf);
    w.interface.writeAll(bytes) catch return;
    w.interface.flush() catch return;
}

const PongOnce = struct {
    io: std.Io,
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *PongOnce) !void {
        self.io = common.globalIo();
        const lp = try test_port.listenLoopback(self.io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *PongOnce) void {
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch |e|
            std.debug.panic("loopback parseIp4 must not fail: {}", .{e});
        if (addr.connect(self.io, .{ .mode = .stream })) |s| s.close(self.io) else |_| {}
        self.thread.join();
        self.server.deinit(self.io);
    }

    fn loop(self: *PongOnce) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept(self.io) catch return;
            if (self.stop.load(.acquire)) {
                conn.close(self.io);
                return;
            }
            const n = fakeReadOnce(self.io, conn);
            if (n > 0) fakeWriteAll(self.io, conn, "+PONG\r\n");
            while (!self.stop.load(.acquire)) common.sleepNanos(5 * std.time.ns_per_ms);
            conn.close(self.io);
        }
    }

    fn config(self: *PongOnce, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, "127.0.0.1");
        return .{ .host = host, .port = self.port, .use_tls = false };
    }
};

test "init(.blocking_consumer) yields a live fd; deinit zeros it" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .blocking_consumer);
    try std.testing.expectEqual(Connection.Role.blocking_consumer, conn.role);
    try std.testing.expect(conn.fd != Connection.INVALID_FD);
    conn.deinit();
    try std.testing.expectEqual(Connection.INVALID_FD, conn.fd);
}

test "init(.subscriber) sets role and dials cleanly" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .subscriber);
    defer conn.deinit();
    try std.testing.expectEqual(Connection.Role.subscriber, conn.role);
}

test "command round-trips a single RESP reply" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    defer conn.deinit();

    var resp = try conn.command(&.{"PING"});
    defer resp.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("PONG", resp.simple);
}

test "post-deinit Connection has INVALID_FD and .closed state — IO call paths assert in debug" {
    // Spec invariant 13: after `deinit`, every subsequent IO path must
    // assert. The IO entry points (`command` → `commandAllowError`) start
    // with `std.debug.assert(self.fd != INVALID_FD)` and
    // `std.debug.assert(self.state == .active)`. We can't catch panics in
    // a zig 0.15 unit test (no `expectPanic`), but we CAN verify both
    // guarded preconditions hold post-deinit — calling `commandAllowError`
    // here would fire either assert deterministically in debug builds.
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    try std.testing.expect(conn.fd != Connection.INVALID_FD);
    try std.testing.expect(conn.state == .active);

    conn.deinit();

    try std.testing.expectEqual(Connection.INVALID_FD, conn.fd);
    try std.testing.expect(conn.state == .closed);
}

test "post-deinit Connection has .closed state — double-deinit asserts in debug" {
    // Spec invariant 14: `.closed` is reachable exactly once per Connection
    // lifetime. `deinit`'s state switch lists `.closed => std.debug.assert(false)` —
    // a second call would fire that assert in debug. Same constraint as the
    // IO test above: we can't catch the panic from inside the same process,
    // so we verify the post-first-deinit state is `.closed`, which is the
    // precondition the second-deinit assert checks. `transitionTo` itself
    // guards `.closed` as a terminal state (Invariant 14): no transition
    // out of `.closed` is legal, so even bypassing deinit's switch via the
    // direct mutation path lands on the same assertion gate.
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    try std.testing.expect(conn.state == .active);

    conn.deinit();
    try std.testing.expect(conn.state == .closed);
    // A subsequent `conn.deinit()` would hit `.closed => std.debug.assert(false)`
    // at redis_connection.zig and panic the test runner; the single-transition
    // invariant is enforced by the precondition observable here.
}

// Serves `+PONG\r\n+EXTRA\r\n` in a single write — the parser consumes
// `+PONG\r\n` and the trailing 8 bytes stay buffered, simulating a server
// that flushed bytes past the expected reply boundary. Used to prove the
// residual-byte poison check at `commandAllowError`.
const ResidualBytesAfterReply = struct {
    io: std.Io,
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *ResidualBytesAfterReply) !void {
        self.io = common.globalIo();
        const lp = try test_port.listenLoopback(self.io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *ResidualBytesAfterReply) void {
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4(LOOPBACK_IPV4, self.port) catch |e|
            std.debug.panic("loopback parseIp4 must not fail: {}", .{e});
        if (addr.connect(self.io, .{ .mode = .stream })) |s| s.close(self.io) else |_| {}
        self.thread.join();
        self.server.deinit(self.io);
    }

    fn loop(self: *ResidualBytesAfterReply) void {
        const conn = self.server.accept(self.io) catch return;
        if (self.stop.load(.acquire)) {
            conn.close(self.io);
            return;
        }
        const n = fakeReadOnce(self.io, conn);
        // One writeAll → kernel coalesces into a single TCP segment on
        // loopback; the std.Io.Reader pulls both frames into its buffer
        // in one read syscall. Parser consumes `+PONG\r\n`; `+EXTRA\r\n`
        // remains buffered. bufferedLen() > 0 → poison.
        // pin test: literal is the contract — the parser MUST consume
        // `+PONG\r\n` and leave the trailing 8 bytes (`+EXTRA\r\n`)
        // buffered. Any rewrite that changes the byte boundary breaks
        // the residual-byte check the production code asserts on.
        if (n > 0) fakeWriteAll(self.io, conn, "+PONG\r\n+EXTRA\r\n");
        while (!self.stop.load(.acquire)) common.sleepNanos(FAKE_SHUTDOWN_POLL_NS);
        conn.close(self.io);
    }

    fn config(self: *ResidualBytesAfterReply, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_IPV4);
        return .{ .host = host, .port = self.port, .use_tls = false };
    }
};

// Writes the length header of a bulk-string reply (`$5\r\n`) and then
// hangs — never sends the 5-byte body. With `SO_RCVTIMEO` armed via
// `applyReadTimeout`, the parser blocks on body read until the kernel
// returns EAGAIN, which surfaces as `error.ReadFailed`. Proves the
// partial-frame timeout path poisons the conn (spec §protocol-desync).
const PartialFrameThenHang = struct {
    io: std.Io,
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *PartialFrameThenHang) !void {
        self.io = common.globalIo();
        const lp = try test_port.listenLoopback(self.io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *PartialFrameThenHang) void {
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4(LOOPBACK_IPV4, self.port) catch |e|
            std.debug.panic("loopback parseIp4 must not fail: {}", .{e});
        if (addr.connect(self.io, .{ .mode = .stream })) |s| s.close(self.io) else |_| {}
        self.thread.join();
        self.server.deinit(self.io);
    }

    fn loop(self: *PartialFrameThenHang) void {
        const conn = self.server.accept(self.io) catch return;
        if (self.stop.load(.acquire)) {
            conn.close(self.io);
            return;
        }
        const n = fakeReadOnce(self.io, conn);
        // pin test: literal is the contract — `$5\r\n` announces a
        // 5-byte body; never send it. The parser consumes the length
        // header, then blocks on body read until `SO_RCVTIMEO` fires.
        // Rewriting the header (e.g. `$10\r\n`) shifts the parser state
        // and changes which assertion the test is making.
        if (n > 0) fakeWriteAll(self.io, conn, "$5\r\n");
        while (!self.stop.load(.acquire)) common.sleepNanos(FAKE_SHUTDOWN_POLL_NS);
        conn.close(self.io);
    }

    fn config(self: *PartialFrameThenHang, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_IPV4);
        return .{ .host = host, .port = self.port, .use_tls = false };
    }
};

test "commandAllowError: partial RESP frame past read timeout surfaces RedisRequestTimeout and poisons conn" {
    var srv: PartialFrameThenHang = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    defer conn.deinit();

    // Arm the read timeout BEFORE issuing the command — the fake hangs
    // after writing `$5\r\n`, so without the timeout the parser would
    // block forever on the missing body. 200ms keeps the test fast while
    // remaining well above poll(2) granularity.
    conn.applyReadTimeout(200);

    const start = clock.nowNanos();
    const result = conn.commandAllowError(&.{"PING"});
    const elapsed_ns = clock.nowNanos() - start;

    try std.testing.expectError(error.RedisRequestTimeout, result);

    // Load-bearing assertions:
    //   1. Partial-frame parser failure poisons the conn — Pool.release
    //      with ok=false closes; the retry layer dials fresh.
    //   2. Timing floor proves SO_RCVTIMEO actually fired (not a same-tick
    //      EOF from peer close). Upper bound catches retry / spin bugs.
    try std.testing.expect(conn.state == .poisoned);
    try std.testing.expect(elapsed_ns >= 100 * std.time.ns_per_ms);
    try std.testing.expect(elapsed_ns < RCVTIMEO_CEILING_NS);
}

test "commandAllowError: residual bytes after parsed reply poison the conn and surface RedisProtocolDesync" {
    var srv: ResidualBytesAfterReply = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    defer conn.deinit();

    const result = conn.commandAllowError(&.{"PING"});
    try std.testing.expectError(error.RedisProtocolDesync, result);

    // Load-bearing assertions:
    //   1. The conn transitioned to .poisoned — Pool.release(conn, true)
    //      will see this and close the conn instead of parking back to idle.
    //   2. The parsed RespValue was freed before the error return —
    //      `std.testing.allocator` would flag the leak if not.
    try std.testing.expect(conn.state == .poisoned);
}

// ── #14: redis_err logged before deinit ────────────────────────────────
//
// Fake that returns a single `.err` RESP frame, then keeps the socket
// open. Drives `command()` into its resumable-error branch where
// `log.warn("redis_command_err_reply", .{ .cmd, .server_err })` fires.
// BufferedSink captures the emitted line so the test can assert the
// log carries the server's error text — pre-sink-refactor this surface
// was impossible to assert on (agentsfleetdLog hardcoded stderr.writeAll).
const WrongtypeErrReply = struct {
    io: std.Io,
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *WrongtypeErrReply) !void {
        self.io = common.globalIo();
        const lp = try test_port.listenLoopback(self.io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *WrongtypeErrReply) void {
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4(LOOPBACK_IPV4, self.port) catch |e|
            std.debug.panic("loopback parseIp4 must not fail: {}", .{e});
        if (addr.connect(self.io, .{ .mode = .stream })) |s| s.close(self.io) else |_| {}
        self.thread.join();
        self.server.deinit(self.io);
    }

    fn loop(self: *WrongtypeErrReply) void {
        const conn = self.server.accept(self.io) catch return;
        if (self.stop.load(.acquire)) {
            conn.close(self.io);
            return;
        }
        const n = fakeReadOnce(self.io, conn);
        // pin test: literal is the contract — RESP `-WRONGTYPE` is the
        // canonical resumable error for a type-mismatched op. The leading
        // `-` plus the trailing CRLF make this a complete RESP frame so
        // the parser returns `.err`; anything else would short-read.
        if (n > 0) fakeWriteAll(self.io, conn, "-WRONGTYPE Operation against a key holding the wrong kind of value\r\n");
        while (!self.stop.load(.acquire)) common.sleepNanos(FAKE_SHUTDOWN_POLL_NS);
        conn.close(self.io);
    }

    fn config(self: *WrongtypeErrReply, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_IPV4);
        return .{ .host = host, .port = self.port, .use_tls = false };
    }
};

test "command on resumable .err reply emits redis_command_err_reply warn carrying server text" {
    var srv: WrongtypeErrReply = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    // Install the buffered sink BEFORE the conn issues commands, clear
    // on the way out so downstream tests don't observe stale captures.
    var bs = log_sinks.BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    log_sinks.clearSinksForTest();
    defer log_sinks.clearSinksForTest();
    log_sinks.registerSink(bs.sink());

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    defer conn.deinit();

    const result = conn.command(&.{ "LPUSH", "any-key", "v" });
    try std.testing.expectError(error.RedisCommandError, result);

    // Load-bearing captures: the warn line fires from
    // redis_connection.zig's resumable-error branch BEFORE deinit. With
    // the sink in place, we can prove the operator-visible signal made
    // it through the format pipeline. snapshot returns an owned copy
    // (race-safe vs concurrent emits); caller frees with bs.alloc.
    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "redis_command_err_reply") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "WRONGTYPE") != null);
    // Conn stayed in protocol sync after the .err reply (resumable):
    // a Pool would park it back to idle; the next command on this conn
    // would reuse the socket.
    try std.testing.expect(conn.state == .active);
}

// ── #15: TLS handshake failure clean teardown ──────────────────────────
//
// Fake server that completes the TCP `accept` then writes garbage bytes
// where a TLS ServerHello belongs. The TLS ClientHello on `Connection.init`
// fails parsing the response; `init` propagates the error, the partial
// transport is torn down, and `std.testing.allocator` confirms no leak.
//
// `redis_transport.zig`'s `tls.initInPlace` calls into `std.crypto.tls.Client.init`
// — that's the path the bad bytes hit. The exact error variant depends on
// where parsing fails inside std.crypto.tls; we assert the init returns
// SOMETHING (not specific code) plus a clean leak audit.
const TlsGarbageHandshake = struct {
    io: std.Io,
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *TlsGarbageHandshake) !void {
        self.io = common.globalIo();
        const lp = try test_port.listenLoopback(self.io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *TlsGarbageHandshake) void {
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4(LOOPBACK_IPV4, self.port) catch |e|
            std.debug.panic("loopback parseIp4 must not fail: {}", .{e});
        if (addr.connect(self.io, .{ .mode = .stream })) |s| s.close(self.io) else |_| {}
        self.thread.join();
        self.server.deinit(self.io);
    }

    fn loop(self: *TlsGarbageHandshake) void {
        const conn = self.server.accept(self.io) catch return;
        if (self.stop.load(.acquire)) {
            conn.close(self.io);
            return;
        }
        // Drain whatever ClientHello bytes the client sent, then reply
        // with 64 bytes of garbage — not a TLS record header, not even
        // a coherent protocol. std.crypto.tls's record parser will
        // reject this during the first read of the ServerHello.
        // Drain ClientHello; partial reads / EOF are expected on a fake
        // that returns garbage — we want the WRITE below to fire regardless.
        _ = fakeReadOnce(self.io, conn);
        const garbage = "GARBAGE-NOT-A-TLS-SERVERHELLO-FRAME-AT-ALL-NO-RECORD-LAYER-XYZ\n";
        fakeWriteAll(self.io, conn, garbage);
        // Close IMMEDIATELY after the garbage. Without this, `std.crypto.tls`
        // can block forever on a subsequent record read (it's reading
        // length-prefixed records and the partial garbage doesn't terminate
        // a valid record). Closing forces EOF on the next read, which the
        // TLS client surfaces as a hard error — the behavior under test.
        conn.close(self.io);
    }

    fn config(self: *TlsGarbageHandshake, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_IPV4);
        // `use_tls = true` forces Connection.init through the TLS
        // handshake path, which is the surface under test. The host
        // string doesn't need to match a real cert SNI — the handshake
        // fails on the server's malformed reply long before name check.
        return .{ .host = host, .port = self.port, .use_tls = true };
    }
};

test "Connection.init: TLS handshake against garbage server errors cleanly with no leak" {
    var srv: TlsGarbageHandshake = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var bs = log_sinks.BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    log_sinks.clearSinksForTest();
    defer log_sinks.clearSinksForTest();
    log_sinks.registerSink(bs.sink());

    // `std.testing.allocator` is leak-detecting — if `init`'s errdefer
    // chain misses any allocation, the test runner reports the unfreed
    // bytes on shutdown. We accept ANY error (the specific variant
    // depends on std.crypto.tls internals and isn't load-bearing); the
    // load-bearing assertion is the leak audit.
    const result = Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    try std.testing.expect(std.meta.isError(result));
    // If init succeeded somehow (unexpected), deinit to avoid leaking.
    if (result) |*c| {
        @constCast(c).deinit();
    } else |_| {}
}

test "commandAllowError: OOM during RESP read propagates and poisons the connection" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(srv.io, std.testing.allocator, &cfg, .pooled);
    // Restore the real allocator before deinit so transport cleanup uses
    // the same allocator that init dialed with.
    defer {
        conn.alloc = std.testing.allocator;
        conn.deinit();
    }

    // FailingAllocator with fail_index=0 returns OutOfMemory on every
    // allocation. The argv write path doesn't allocate, so the WRITE
    // round-trip completes; readRespValue's first allocation inside the
    // RESP parser fails.
    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    conn.alloc = fa.allocator();

    const result = conn.commandAllowError(&.{"PING"});
    try std.testing.expectError(error.OutOfMemory, result);

    // Load-bearing assertion: OOM during RESP read must poison the
    // connection because bulk-string (`$N\r\n…`) and array (`*N\r\n…`)
    // replies consume the length/count header before the body allocation
    // fires — partial bytes remain in the transport buffer and the next
    // RESP read would start mid-message. The OOM still surfaces verbatim
    // (no `ReadFailed` re-tag) AND Pool.release(conn, true) sees the
    // poisoned state and closes the conn.
    try std.testing.expect(conn.state == .poisoned);
}
