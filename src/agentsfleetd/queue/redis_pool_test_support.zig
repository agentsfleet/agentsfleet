//! Shared in-process RESP fake-server scaffolding for the redis `Pool` tests.
//!
//! Extracted so both `redis_pool_test.zig` and `redis_pool_concurrency_test.zig`
//! drive connections against the same deterministic PING fake (no real Redis
//! needed) without duplicating the socket plumbing. Every `pub` decl here is
//! consumed by one or both of those test files.

const std = @import("std");
const common = @import("common");
const LOOPBACK_HOST = "127.0.0.1";
const net = std.Io.net;
const test_port = @import("../http/test_port.zig");
const redis_config = @import("redis_config.zig");
const PONG_2 = "+PONG\r\n";

// ── Zig 0.16 socket-fake plumbing ──────────────────────────────────────
// `std.Io.net.Stream` dropped the direct `read`/`writeAll`/`close` methods;
// reads/writes now travel through buffered `Reader`/`Writer` interfaces and
// `close` takes an `io`. These helpers keep the fake servers faithful to the
// removed primitives: one recv per call (content ignored — callers only branch
// on byte-count vs EOF) and a write+flush that swallows peer-side errors
// exactly as the old `writeAll(...) catch {}` did.
pub fn streamReadOnce(r: *std.Io.Reader) usize {
    var scratch: [256]u8 = undefined;
    var data: [1][]u8 = .{&scratch};
    return r.readVec(&data) catch 0;
}

pub fn streamWriteAll(w: *std.Io.Writer, bytes: []const u8) void {
    w.writeAll(bytes) catch return;
    w.flush() catch return;
}

// Redial the loopback fake on `port` to unblock its parked `accept`, then drop
// the throwaway stream. Replaces the removed `std.net.tcpConnectToAddress`.
pub fn pokeAccept(io: std.Io, port: u16) void {
    var addr = net.IpAddress.parseIp4(LOOPBACK_HOST, port) catch return;
    if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
}

/// In-process RESP fake that answers PING with `+PONG`. `keep_open=true`
/// repeat-serves a PONG for every command on the socket (so a pooled conn can
/// be parked and reused); `keep_open=false` serves exactly one reply then
/// closes. Deterministic across CI environments — no real Redis.
pub const PingFake = struct {
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),
    keep_open: bool,
    /// The connection currently being served, so `shutdown` can reach a read
    /// already in flight. `pokeAccept` only ever wakes the LISTEN socket; a
    /// thread parked in `readv` on an accepted conn is deaf to it, and
    /// `join` then blocks forever. Guarded rather than atomic because the
    /// serving thread clears it before closing, and shutdown must not touch
    /// a handle that close has already released.
    live_mu: common.Mutex,
    live: ?net.Stream,

    pub fn start(self: *PingFake, keep_open: bool) !void {
        const lp = try test_port.listenLoopback(common.globalIo());
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.keep_open = keep_open;
        self.live_mu = .{};
        self.live = null;
        self.thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    pub fn shutdown(self: *PingFake) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        // Half-close the read side of the live conn (never `close`): the
        // parked `readv` returns EOF, the serve loop sees it and exits, and
        // the fd stays valid so the serving thread's own `close` remains the
        // only one. A pool test that parks an idle conn — the saturated-cap
        // suite does exactly that — otherwise wedges here forever.
        self.live_mu.lock();
        // A peer that already hung up makes this a no-op, which is the
        // success case for our purposes: the parked read is over either way.
        if (self.live) |stream| stream.shutdown(io, .recv) catch |err|
            std.log.debug("ping fake read-shutdown ignored: {s}", .{@errorName(err)});
        self.live_mu.unlock();
        pokeAccept(io, self.port);
        self.thread.join();
        self.server.deinit(io);
    }

    fn acceptLoop(self: *PingFake) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            var stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);
            self.publishLive(stream);

            var rbuf: [256]u8 = undefined;
            var wbuf: [64]u8 = undefined;
            var sr = stream.reader(io, &rbuf);
            var sw = stream.writer(io, &wbuf);
            const r = &sr.interface;
            const w = &sw.interface;
            if (self.keep_open) {
                // Repeat-serve PONG for every command issued on this socket so
                // a parked-then-reused conn (release(ok=true) → acquire) is
                // answered. Loop until peer-close, shutdown, or short-read EOF.
                while (!self.stop.load(.acquire)) {
                    if (streamReadOnce(r) == 0) break;
                    streamWriteAll(w, PONG_2);
                }
            } else {
                // Single-shot: read one command, write one reply, close.
                if (streamReadOnce(r) > 0) streamWriteAll(w, PONG_2);
            }
            // Retract before closing, so a concurrent `shutdown` can never
            // reach this handle after the close below releases it.
            self.retractLive();
            stream.close(io);
        }
    }

    fn publishLive(self: *PingFake, stream: net.Stream) void {
        self.live_mu.lock();
        defer self.live_mu.unlock();
        self.live = stream;
    }

    fn retractLive(self: *PingFake) void {
        self.live_mu.lock();
        defer self.live_mu.unlock();
        self.live = null;
    }

    pub fn config(self: *PingFake, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_HOST);
        return .{
            .host = host,
            .port = self.port,
            .use_tls = false,
        };
    }
};
