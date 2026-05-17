//! Integration tests for the unified `Subscriber` (slice 4) — exercises
//! `connectFromUrl` against a real Redis broker so the `SO_RCVTIMEO`
//! install-after-subscribe-ack path and the `nextMessage → null` swallow
//! on read timeout are covered end-to-end. Unit-shape coverage of the
//! subscribe-ack parser lives inside `redis_subscriber.zig`; the broker
//! consumers in `src/zombie/event_loop_harness_*_test.zig` exercise the
//! blocking-mode `.{ .read_timeout_ms = N }` path under a real workload.
//!
//! Skip-by-default unless `TEST_REDIS_TLS_URL=rediss://...` is exported.
//! Pattern matches `redis_test.zig` "integration: rediss ping".

const std = @import("std");
const Subscriber = @import("redis_subscriber.zig");
const redis = @import("redis.zig");

// Loopback host shared across every in-process fake server in this file.
// `parseIp4` (for `listen`) and `alloc.dupe` (for `Config.host`) both want
// the same literal — extracting prevents drift.
const LOOPBACK_IPV4 = "127.0.0.1";

// Shutdown-loop poll tick for fake servers — fine-grained enough that
// `stop.store(.release)` is observed within one tick of `shutdown()` but
// coarse enough not to peg a core during a long-running test.
const FAKE_SHUTDOWN_POLL_NS = 10 * std.time.ns_per_ms;

// Bounds for `SO_RCVTIMEO`-fired-or-equivalent timing assertions:
//   - floor: anything below means the kernel returned EAGAIN immediately
//     (peer close disguised as timeout) or the test never armed the
//     timeout in the first place.
//   - ceiling: catches retry-loop / busy-spin bugs that would let an
//     "expected ~100ms" test silently hang for minutes.
const RCVTIMEO_FLOOR_NS = 50 * std.time.ns_per_ms;
const RCVTIMEO_CEILING_NS = 5 * std.time.ns_per_s;

// Façade-vs-impl type-equality: the unified façade `redis.zig` MUST
// re-export `redis_subscriber.zig` as `Subscriber`. The previous split
// `redis_pubsub.zig` is gone; the only public entry into pub/sub is via
// `redis.Subscriber`. A future refactor that introduces a wrapper struct
// (and re-routes consumers) would shadow this assertion — the failure is
// the warning shot to update the spec's "facade unified" claim too.
comptime {
    std.debug.assert(redis.Subscriber == Subscriber);
}

// Deletion guard for the retired `redis_pubsub.zig`. The spec's "unify"
// claim hinges on there being exactly one Subscriber implementation; a
// re-introduction would silently fork the surface. CWD during
// `zig build test` is the project root, so the relative path resolves
// against the workspace toplevel. `error.FileNotFound` is the green path.
test "redis_pubsub.zig stays deleted (spec: unified subscriber)" {
    const result = std.fs.cwd().access("src/queue/redis_pubsub.zig", .{});
    if (result) |_| {
        std.debug.print("FAIL: src/queue/redis_pubsub.zig was re-introduced — spec §unified-subscriber claim is broken\n", .{});
        return error.RedisPubsubResurrected;
    } else |err| {
        try std.testing.expectEqual(error.FileNotFound, err);
    }
}

const TLS_URL_ENV = "TEST_REDIS_TLS_URL";
const REDISS_SCHEME = "rediss://";

fn tlsUrlOrSkip(alloc: std.mem.Allocator) ![]u8 {
    const url = std.process.getEnvVarOwned(alloc, TLS_URL_ENV) catch return error.SkipZigTest;
    if (!std.mem.startsWith(u8, url, REDISS_SCHEME)) {
        alloc.free(url);
        return error.SkipZigTest;
    }
    return url;
}

// In-process fake that:
//   1. accepts one TCP connection
//   2. drains the SUBSCRIBE command bytes (assumes no AUTH precedes it —
//      the test URL must be credential-less; otherwise the first `read()`
//      consumes AUTH instead of SUBSCRIBE and the subscriber hangs)
//   3. writes a `*3\r\n$9\r\nsubscribe\r\n$<n>\r\n<chan>\r\n:1\r\n` ack
//   4. sleeps `publish_delay_ms` (simulates a publisher pushing after a delay)
//   5. writes a `*3\r\n$7\r\nmessage\r\n$<n>\r\n<chan>\r\n$<n>\r\n<payload>\r\n` push
//   6. holds the socket open until shutdown
//
// Used to prove `nextMessage()` with `read_timeout_ms = null` blocks past
// the ack and returns the delivered Message — the production blocking path.
const SubscribeAckThenMessage = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    channel: []const u8,
    payload: []const u8,
    publish_delay_ms: u64,

    fn start(self: *SubscribeAckThenMessage, channel: []const u8, payload: []const u8, publish_delay_ms: u64) !void {
        const loopback = try std.net.Address.parseIp4(LOOPBACK_IPV4, 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.channel = channel;
        self.payload = payload;
        self.publish_delay_ms = publish_delay_ms;
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *SubscribeAckThenMessage) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *SubscribeAckThenMessage) void {
        const conn = self.server.accept() catch return;
        if (self.stop.load(.acquire)) {
            conn.stream.close();
            return;
        }
        var drain: [256]u8 = undefined;
        _ = conn.stream.read(&drain) catch {};

        var ack_buf: [256]u8 = undefined;
        const ack = std.fmt.bufPrint(
            &ack_buf,
            "*3\r\n$9\r\nsubscribe\r\n${d}\r\n{s}\r\n:1\r\n",
            .{ self.channel.len, self.channel },
        ) catch {
            conn.stream.close();
            return;
        };
        conn.stream.writeAll(ack) catch {
            conn.stream.close();
            return;
        };

        std.Thread.sleep(self.publish_delay_ms * std.time.ns_per_ms);

        var msg_buf: [512]u8 = undefined;
        const msg = std.fmt.bufPrint(
            &msg_buf,
            "*3\r\n$7\r\nmessage\r\n${d}\r\n{s}\r\n${d}\r\n{s}\r\n",
            .{ self.channel.len, self.channel, self.payload.len, self.payload },
        ) catch {
            conn.stream.close();
            return;
        };
        conn.stream.writeAll(msg) catch {};

        while (!self.stop.load(.acquire)) std.Thread.sleep(FAKE_SHUTDOWN_POLL_NS);
        conn.stream.close();
    }

    fn url(self: *SubscribeAckThenMessage, alloc: std.mem.Allocator) ![]u8 {
        return try std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.addr.in.getPort()});
    }
};

test "Subscriber.nextMessage with read_timeout_ms=null blocks until message arrives" {
    // Spec: production path uses `read_timeout_ms = null` (block forever).
    // The fake delays its `message` push by 100ms after the SUBSCRIBE ack;
    // `nextMessage()` must NOT return null in that window (no SO_RCVTIMEO
    // armed → read blocks), and must return the published payload when the
    // server eventually pushes it.
    const alloc = std.testing.allocator;
    const channel = "test:sub:blocking";
    const payload = "hello-blocking-world";
    const publish_delay_ms: u64 = 100;

    var fake: SubscribeAckThenMessage = undefined;
    try fake.start(channel, payload, publish_delay_ms);
    defer fake.shutdown();

    const url = try fake.url(alloc);
    defer alloc.free(url);

    var sub = try Subscriber.connectFromUrl(alloc, url, .{ .read_timeout_ms = null });
    defer sub.deinit();
    try sub.subscribe(channel);

    const start = std.time.nanoTimestamp();
    const maybe_msg = try sub.nextMessage();
    const elapsed_ns = std.time.nanoTimestamp() - start;

    // Must have a message — null would mean stream closed or timeout fired.
    try std.testing.expect(maybe_msg != null);
    var got = maybe_msg.?;
    defer got.deinit(alloc);
    try std.testing.expectEqualStrings(channel, got.channel);
    try std.testing.expectEqualStrings(payload, got.payload);

    // Elapsed proves we actually blocked through the publisher's delay
    // (lower bound generous for CI jitter; upper bound catches retry / spin
    // bugs in the read loop).
    try std.testing.expect(elapsed_ns >= RCVTIMEO_FLOOR_NS);
    try std.testing.expect(elapsed_ns < RCVTIMEO_CEILING_NS);
}

test "integration: subscriber receives a real PUBLISH from a sibling connection" {
    // Real-Redis sibling for the in-process #9 unit test. Proves the
    // SUBSCRIBE ack handshake + message-frame parsing against the real
    // server's wire format (not a hand-crafted fake). A publisher Client
    // on a separate connection PUBLISHes after 100ms; the subscriber
    // blocks past the ack and returns the delivered payload.
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    var channel_buf: [64]u8 = undefined;
    const channel = try std.fmt.bufPrint(&channel_buf, "uz:m69:pub:{d}", .{std.time.nanoTimestamp()});
    const payload = "real-redis-publish";

    var sub = try Subscriber.connectFromUrl(alloc, tls_url, .{ .read_timeout_ms = null });
    defer sub.deinit();
    try sub.subscribe(channel);

    // Publisher runs in a sibling thread on its own Client so the SUBSCRIBE
    // and PUBLISH ride independent TCP connections (the SUBSCRIBE socket
    // can't issue commands besides UNSUBSCRIBE per Redis pub/sub semantics).
    const PublishCtx = struct {
        url: []const u8,
        channel: []const u8,
        payload: []const u8,
        alloc: std.mem.Allocator,
        delay_ms: u64,

        fn run(ctx: *const @This()) void {
            std.Thread.sleep(ctx.delay_ms * std.time.ns_per_ms);
            const Client = @import("redis_client.zig");
            var client = Client.connectFromUrl(ctx.alloc, ctx.url) catch return;
            defer client.deinit();
            client.publish(ctx.channel, ctx.payload) catch {};
        }
    };
    var ctx = PublishCtx{
        .url = tls_url,
        .channel = channel,
        .payload = payload,
        .alloc = alloc,
        .delay_ms = 100,
    };
    const pub_thread = try std.Thread.spawn(.{}, PublishCtx.run, .{&ctx});

    const start = std.time.nanoTimestamp();
    const maybe_msg = try sub.nextMessage();
    const elapsed_ns = std.time.nanoTimestamp() - start;
    pub_thread.join();

    try std.testing.expect(maybe_msg != null);
    var got = maybe_msg.?;
    defer got.deinit(alloc);
    try std.testing.expectEqualStrings(channel, got.channel);
    try std.testing.expectEqualStrings(payload, got.payload);

    try std.testing.expect(elapsed_ns >= RCVTIMEO_FLOOR_NS);
    try std.testing.expect(elapsed_ns < RCVTIMEO_CEILING_NS);
}

test "integration: subscriber with 100ms read_timeout returns null on a quiet channel" {
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    var sub = try Subscriber.connectFromUrl(alloc, tls_url, .{ .read_timeout_ms = 100 });
    defer sub.deinit();

    // Channel name unique enough that no other test or live worker would
    // PUBLISH against it during this run. The subscribe-ack handshake must
    // complete (it has no SO_RCVTIMEO in flight yet — set post-ack only)
    // before we sit on nextMessage.
    try sub.subscribe("test:subscriber:quiet-channel");

    const start = std.time.nanoTimestamp();
    const msg = try sub.nextMessage();
    const elapsed_ns = std.time.nanoTimestamp() - start;

    try std.testing.expect(msg == null);
    // Timer floor: kernel SO_RCVTIMEO granularity + RESP parser overhead
    // typically lands ≥50ms when the budget is 100ms. Generous upper bound
    // (5s) prevents this from flaking on a loaded CI host; the budget tells
    // us "fired roughly on time" not "fired exactly at 100ms".
    try std.testing.expect(elapsed_ns >= RCVTIMEO_FLOOR_NS);
    try std.testing.expect(elapsed_ns < RCVTIMEO_CEILING_NS);
}

// ── #26: subscriber disconnect storm — 50 subscribers, no leak ─────────
//
// 50 subscribers connect to an in-process fake server in parallel
// std.Threads, each subscribed to a channel. The fake accepts every
// connection, sends a subscribe-ack, then closes the socket
// (simulating broker death). Every nextMessage() returns null
// (timeout/EOF) cleanly; every deinit() runs without leaking;
// std.testing.allocator's scope-exit audit is the load-bearing
// assertion. Trimmed from 100 to 50 to keep CPU-starved CI hosts
// from flaking on thread-spawn pile-up.
const AckThenCloseStorm = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *AckThenCloseStorm) !void {
        const loopback = try std.net.Address.parseIp4(LOOPBACK_IPV4, 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *AckThenCloseStorm) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *AckThenCloseStorm) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept() catch return;
            if (self.stop.load(.acquire)) {
                conn.stream.close();
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);
            var buf: [256]u8 = undefined;
            _ = conn.stream.read(&buf) catch {};
            // pin test: literal is the contract — a valid RESP
            // subscribe-ack array for channel "ch". The subscriber's
            // ack parser advances past this; the immediate close
            // afterwards surfaces as null on nextMessage.
            conn.stream.writeAll("*3\r\n$9\r\nsubscribe\r\n$2\r\nch\r\n:1\r\n") catch {};
            conn.stream.close();
        }
    }

    fn url(self: *AckThenCloseStorm, alloc: std.mem.Allocator) ![]u8 {
        return try std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.addr.in.getPort()});
    }
};

const SubscriberStormCtx = struct {
    url: []const u8,
    alloc: std.mem.Allocator,
    ok: *std.atomic.Value(u32),

    fn worker(self: *SubscriberStormCtx) void {
        var sub = Subscriber.connectFromUrl(self.alloc, self.url, .{ .read_timeout_ms = 200 }) catch return;
        defer sub.deinit();
        sub.subscribe("ch") catch return;
        if (sub.nextMessage()) |maybe| {
            if (maybe) |m| {
                var mm = m;
                mm.deinit(self.alloc);
            }
        } else |_| {}
        _ = self.ok.fetchAdd(1, .monotonic);
    }
};

test "Subscriber storm: 50 subscribers against a closing broker — no leak, all teardown clean" {
    const alloc = std.testing.allocator;

    var fake: AckThenCloseStorm = undefined;
    try fake.start();
    defer fake.shutdown();

    const url = try fake.url(alloc);
    defer alloc.free(url);

    var ok_count = std.atomic.Value(u32).init(0);
    var ctx = SubscriberStormCtx{ .url = url, .alloc = alloc, .ok = &ok_count };

    var threads: [50]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, SubscriberStormCtx.worker, .{&ctx});
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u32, 50), ok_count.load(.monotonic));
    try std.testing.expectEqual(@as(u32, 50), fake.accepts.load(.monotonic));
}
