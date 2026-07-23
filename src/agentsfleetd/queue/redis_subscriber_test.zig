//! Integration tests for the unified `Subscriber` (slice 4) — exercises
//! `connectFromUrl` against a real Redis broker so the `SO_RCVTIMEO`
//! install-after-subscribe-ack path and the `nextMessage → null` swallow
//! on read timeout are covered end-to-end. Unit-shape coverage of the
//! subscribe-ack parser lives inside `redis_subscriber.zig`; the tests below
//! exercise both blocking and timed `read_timeout_ms` paths under a real workload.
//!
//! Skip-by-default unless `TEST_REDIS_TLS_URL=rediss://...` is exported.
//! Pattern matches `redis_test.zig` "integration: rediss ping".

const std = @import("std");
const common = @import("common");
const clock = @import("common").clock;
const net = std.Io.net;
const test_port = @import("../http/test_port.zig");
const Subscriber = @import("redis_subscriber.zig");
const redis = @import("redis.zig");
const call_deadline = @import("call_deadline");
const redis_config = @import("redis_config.zig");

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

// Drain whatever the client has sent with a SINGLE read. `readSliceShort`
// loops until the buffer fills or hits EOF, so on an open socket holding only
// the short SUBSCRIBE it blocks forever waiting for bytes that never arrive.
// One `readVec` consumes what is available; the fakes never inspect the bytes.
fn drainOnce(reader: *std.Io.Reader, buf: []u8) void {
    var vec: [1][]u8 = .{buf};
    _ = reader.readVec(&vec) catch return;
}

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
    const result = std.Io.Dir.cwd().access(common.globalIo(), "src/queue/redis_pubsub.zig", .{});
    if (result) |_| {
        std.debug.print("FAIL: src/queue/redis_pubsub.zig was re-introduced — spec §unified-subscriber claim is broken\n", .{});
        return error.RedisPubsubResurrected;
    } else |err| {
        try std.testing.expectEqual(error.FileNotFound, err);
    }
}

const TLS_URL_ENV = "TEST_REDIS_TLS_URL";
const REDISS_SCHEME = "rediss://";

fn tlsUrlOrSkip() ![]const u8 {
    const url = common.env.testLiveValue(TLS_URL_ENV) orelse return error.SkipZigTest;
    if (!std.mem.startsWith(u8, url, REDISS_SCHEME)) {
        return error.SkipZigTest;
    }
    return url;
}

/// Translate TLS handshake / cert-bundle failures from the test
/// environment into skips. `std.crypto.tls.Client.init` surfaces
/// `error.CertificateSignatureInvalid` / `CertificateExpired` /
/// `CertificateHostMismatch` / `CertificateIssuerNotFound` when the CI
/// runner's CA bundle doesn't trust the upstream Redis cert chain —
/// not a failure of our Subscriber code. Real subscriber bugs would
/// fire BEFORE the TLS error path (e.g., bad RESP parse on a successful
/// handshake) and would not match this allow-list.
fn isTlsEnvError(err: anyerror) bool {
    return switch (err) {
        error.CertificateSignatureInvalid,
        error.CertificateExpired,
        error.CertificateHostMismatch,
        error.CertificateIssuerNotFound,
        error.CertificateIssuerMismatch,
        error.CertificateNotYetValid,
        error.TlsAlert,
        error.TlsInitializationFailed,
        error.ConnectionRefused,
        => true,
        else => false,
    };
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
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    channel: []const u8,
    payload: []const u8,
    publish_delay_ms: u64,

    fn start(self: *SubscribeAckThenMessage, channel: []const u8, payload: []const u8, publish_delay_ms: u64) !void {
        const lp = try test_port.listenLoopback(common.globalIo());
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.channel = channel;
        self.payload = payload;
        self.publish_delay_ms = publish_delay_ms;
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *SubscribeAckThenMessage) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.thread.join();
        self.server.deinit(io);
    }

    fn loop(self: *SubscribeAckThenMessage) void {
        const io = common.globalIo();
        const stream = self.server.accept(io) catch return;
        if (self.stop.load(.acquire)) {
            stream.close(io);
            return;
        }
        var read_buf: [256]u8 = undefined;
        var stream_reader = stream.reader(io, &read_buf);
        var drain: [256]u8 = undefined;
        // Drain the SUBSCRIBE with a single read (see `drainOnce`); the fake
        // never inspects the bytes — it just needs the socket cleared before
        // it acks and pushes the message.
        drainOnce(&stream_reader.interface, &drain);

        var write_buf: [512]u8 = undefined;
        var stream_writer = stream.writer(io, &write_buf);
        const w = &stream_writer.interface;

        w.print(
            "*3\r\n$9\r\nsubscribe\r\n${d}\r\n{s}\r\n:1\r\n",
            .{ self.channel.len, self.channel },
        ) catch {
            stream.close(io);
            return;
        };
        w.flush() catch {
            stream.close(io);
            return;
        };

        common.sleepNanos(self.publish_delay_ms * std.time.ns_per_ms);

        w.print(
            "*3\r\n$7\r\nmessage\r\n${d}\r\n{s}\r\n${d}\r\n{s}\r\n",
            .{ self.channel.len, self.channel, self.payload.len, self.payload },
        ) catch {
            stream.close(io);
            return;
        };
        w.flush() catch return;

        while (!self.stop.load(.acquire)) common.sleepNanos(FAKE_SHUTDOWN_POLL_NS);
        stream.close(io);
    }

    fn url(self: *SubscribeAckThenMessage, alloc: std.mem.Allocator) ![]u8 {
        return try std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.port});
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

    var sub = try redis.testing.subscriberFromUrl(common.globalIo(), alloc, url, .{ .read_timeout_ms = null });
    defer sub.deinit();
    try sub.subscribe(channel);

    const start = clock.nowNanos();
    const maybe_msg = try sub.nextMessage();
    const elapsed_ns = clock.nowNanos() - start;

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
    // testLiveValue returns a borrowed slice into the live environment — never free it.
    const tls_url = try tlsUrlOrSkip();

    var channel_buf: [64]u8 = undefined;
    const channel = try std.fmt.bufPrint(&channel_buf, "uz:m69:pub:{d}", .{clock.nowNanos()});
    const payload = "real-redis-publish";

    var sub = redis.testing.subscriberFromUrl(common.globalIo(), alloc, tls_url, .{ .read_timeout_ms = null }) catch |err| {
        if (isTlsEnvError(err)) return error.SkipZigTest;
        return err;
    };
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
            common.sleepNanos(ctx.delay_ms * std.time.ns_per_ms);
            var client = redis.testing.connectFromUrl(common.globalIo(), ctx.alloc, ctx.url) catch return;
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

    const start = clock.nowNanos();
    const maybe_msg = try sub.nextMessage();
    const elapsed_ns = clock.nowNanos() - start;
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
    // testLiveValue returns a borrowed slice into the live environment — never free it.
    const tls_url = try tlsUrlOrSkip();

    var sub = redis.testing.subscriberFromUrl(common.globalIo(), alloc, tls_url, .{ .read_timeout_ms = 100 }) catch |err| {
        if (isTlsEnvError(err)) return error.SkipZigTest;
        return err;
    };
    defer sub.deinit();

    // Channel name unique enough that no other test or live worker would
    // PUBLISH against it during this run. The subscribe-ack handshake must
    // complete (it has no SO_RCVTIMEO in flight yet — set post-ack only)
    // before we sit on nextMessage.
    try sub.subscribe("test:subscriber:quiet-channel");

    const start = clock.nowNanos();
    const msg = try sub.nextMessage();
    const elapsed_ns = clock.nowNanos() - start;

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
    server: net.Server,
    port: u16,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *AckThenCloseStorm) !void {
        const lp = try test_port.listenLoopback(common.globalIo());
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *AckThenCloseStorm) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.thread.join();
        self.server.deinit(io);
    }

    fn loop(self: *AckThenCloseStorm) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);
            var read_buf: [256]u8 = undefined;
            var stream_reader = stream.reader(io, &read_buf);
            var drain: [256]u8 = undefined;
            // Drain whatever the subscriber sent with a single read (see
            // `drainOnce`); the load-bearing assertion is the accept count +
            // ack write below, not the bytes read here.
            drainOnce(&stream_reader.interface, &drain);
            // pin test: literal is the contract — a valid RESP
            // subscribe-ack array for channel "ch". The subscriber's
            // ack parser advances past this; the immediate close
            // afterwards surfaces as null on nextMessage.
            var write_buf: [64]u8 = undefined;
            var stream_writer = stream.writer(io, &write_buf);
            stream_writer.interface.writeAll("*3\r\n$9\r\nsubscribe\r\n$2\r\nch\r\n:1\r\n") catch return;
            stream_writer.interface.flush() catch return;
            stream.close(io);
        }
    }

    fn url(self: *AckThenCloseStorm, alloc: std.mem.Allocator) ![]u8 {
        return try std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.port});
    }
};

const SubscriberStormCtx = struct {
    url: []const u8,
    alloc: std.mem.Allocator,
    ok: *std.atomic.Value(u32),

    fn worker(self: *SubscriberStormCtx) void {
        var sub = redis.testing.subscriberFromUrl(common.globalIo(), self.alloc, self.url, .{ .read_timeout_ms = 200 }) catch return;
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

// ── Redis handshake stages share one absolute deadline ──────────────────────

/// Deterministic stall bound: well over scheduler wake latency, well under the
/// suite timeout — a return near this bound proves the DEADLINE released the
/// caller, not the peer.
const SETUP_STALL_DEADLINE_MS: u31 = 250;
const SETUP_STALL_CEILING_NS: i96 = 5 * std.time.ns_per_s;

/// A broker that completes the TCP handshake and then never speaks RESP: the
/// AUTH reply the subscriber blocks on never arrives. Exits when the fired
/// deadline shuts the subscriber's socket down (read returns 0/error).
const StallBroker = struct {
    loopback: test_port.Loopback,
    io: std.Io,
    thread: ?std.Thread = null,

    fn start(io: std.Io) !StallBroker {
        return .{ .loopback = try test_port.listenLoopback(io), .io = io };
    }

    fn serve(self: *StallBroker) !void {
        self.thread = try std.Thread.spawn(.{}, run, .{self});
    }

    fn run(self: *StallBroker) void {
        const conn = self.loopback.server.accept(self.io) catch return;
        defer conn.close(self.io);
        var buf: [512]u8 = undefined;
        // Drain whatever AUTH bytes arrive, reply with nothing. The loop ends
        // when the deadline's shutdown(2) wakes the peer (n == 0) or errors.
        while (true) {
            const n = std.posix.read(conn.socket.handle, &buf) catch return;
            if (n == 0) return;
        }
    }

    fn deinit(self: *StallBroker) void {
        if (self.thread) |t| t.join();
        self.loopback.server.deinit(self.io);
    }
};

/// The scheduler + owner half of `subscription_hub_wire.connectBounded`,
/// restated here so the test drives `connectFromConfig` exactly as the hub
/// does: one absolute budget, owner armed before any stage runs.
const BoundedAttempt = struct {
    backend: call_deadline.MonotonicBackend = .{},
    sched: ?call_deadline.ProcessScheduler = null,
    owner: call_deadline.SocketOwner = .{},
    generation: u64 = 0,
    guard: ?call_deadline.ProcessScheduler.Guard = null,

    fn arm(self: *BoundedAttempt, io: std.Io, deadline_ms: u31) !i96 {
        self.sched = call_deadline.ProcessScheduler.init(std.testing.allocator, &self.backend);
        try self.sched.?.start();
        self.generation = self.owner.beginAttempt();
        self.guard = try self.sched.?.arm(self.owner.target(self.generation), deadline_ms);
        return std.Io.Clock.boot.now(io).toNanoseconds() + @as(i96, deadline_ms) * std.time.ns_per_ms;
    }

    fn deinit(self: *BoundedAttempt) void {
        self.owner.endAttempt();
        if (self.guard) |g| _ = g.finish();
        if (self.sched) |*s| s.deinit();
    }
};

test "integration: redis handshake stages are deadline bounded" {
    // Dimension 3.1 (deterministic stalls, no live Redis): a peer that accepts
    // and then stalls must release the caller with the CLASSIFIED timeout
    // within the budget — for the stage with no socket yet (dial refusal on an
    // exhausted budget) and for a blocked in-stage read (AUTH). The TLS
    // handshake read blocks on the same owned socket as the AUTH read, so the
    // same fire→shutdown→classify chain bounds it.
    const alloc = std.testing.allocator;
    // The raced dial needs a concurrency-capable Io; common.globalIo() is
    // statically single-threaded and would refuse `Io.Select`.
    var threaded: std.Io.Threaded = .init(alloc, .{});
    defer threaded.deinit();
    const io = threaded.io();

    // Stage: resolve/dial with the budget already spent → refused before any
    // socket exists, still the classified timeout.
    {
        var attempt: BoundedAttempt = .{};
        defer attempt.deinit();
        _ = try attempt.arm(io, SETUP_STALL_DEADLINE_MS);
        const spent_deadline = std.Io.Clock.boot.now(io).toNanoseconds() - 1;
        const cfg = redis_config.Config{ .host = "127.0.0.1", .port = 1, .password = "pw" };
        try std.testing.expectError(Subscriber.SetupError.RedisSetupTimedOut, Subscriber.connectFromConfig(
            io,
            alloc,
            cfg,
            .{ .read_timeout_ms = null },
            .{ .owner = &attempt.owner, .generation = attempt.generation, .deadline_ns = spent_deadline },
        ));
    }

    // Stage: AUTH — the broker never answers; the deadline fire shuts the
    // owned socket down and the surfaced error is the classified timeout, not
    // the raw transport failure the shutdown provoked.
    {
        var broker = try StallBroker.start(io);
        defer broker.deinit();
        try broker.serve();

        var attempt: BoundedAttempt = .{};
        defer attempt.deinit();
        const deadline_ns = try attempt.arm(io, SETUP_STALL_DEADLINE_MS);
        const cfg = redis_config.Config{ .host = "127.0.0.1", .port = broker.loopback.port, .password = "pw" };

        const started_ns = std.Io.Clock.boot.now(io).toNanoseconds();
        try std.testing.expectError(Subscriber.SetupError.RedisSetupTimedOut, Subscriber.connectFromConfig(
            io,
            alloc,
            cfg,
            .{ .read_timeout_ms = null },
            .{ .owner = &attempt.owner, .generation = attempt.generation, .deadline_ns = deadline_ns },
        ));
        const elapsed_ns = std.Io.Clock.boot.now(io).toNanoseconds() - started_ns;
        // Released by the fire, not the suite timeout — and no abandoned
        // worker: broker.deinit() joins its thread, which exits only because
        // the shutdown reached its socket.
        try std.testing.expect(elapsed_ns < SETUP_STALL_CEILING_NS);
    }
}
