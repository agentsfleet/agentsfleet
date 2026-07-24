//! Tests for the broker's production wiring (`serve_broker.zig`) — extracted so
//! the production file stays under the 350-line FLL cap. Covers the metrics
//! sink, the exchange boundary, and the deadline-armed outbound (finding ②:
//! a broker token exchange must never run unbounded — fail closed on a stall or
//! an un-armable watchdog, exactly like the connector layer's `bounded_fetch`).

const std = @import("std");
const common = @import("common");
const call_deadline = @import("call_deadline");
const serve_broker = @import("serve_broker.zig");

const testing = std.testing;
const HttpClientExchange = serve_broker.HttpClientExchange;

test "metricsSink emits without dereferencing its opaque ptr" {
    const sink = serve_broker.metricsSink();
    // ptr is undefined by contract; onMint must never touch it.
    sink.onMint(.{ .integration = "github", .outcome = "ok", .latency_ms = 12, .cache_hit = false });
}

test "exchange wires a post boundary over the client" {
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = common.globalIo(), .sched = &sched };
    const boundary = ex.exchange();
    // The boundary points back at the exchange struct (no network here).
    try testing.expect(boundary.ptr == @as(*anyopaque, @ptrCast(&ex)));
}

test "exchange refuses an unusable URL fail-closed, never fetches unarmed (finding ②)" {
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = common.globalIo(), .sched = &sched };
    const boundary = ex.exchange();
    // pinHandle can't parse this → the call is refused before any bytes are sent.
    const r = boundary.post(testing.allocator, .{ .url = "not a url", .body = "{}" });
    try testing.expectError(error.HttpExchangeFailed, r);
}

// A vendor that never answers: listening without accept(2) completes the TCP
// handshake via the backlog, so pin + send succeed and the read stalls — the
// exact hung-token-endpoint shape the broker deadline exists for.
const STALL_DEADLINE_MS: u31 = 250;
const ELAPSED_BOUND_MS: i64 = 2_000; // well over the deadline, well under suite timeout

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

test "exchange deadline fires on a stalled vendor and fails closed within the bound (finding ②)" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    // Inject the short deadline so the fire returns fast (production is 10 s).
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = io, .sched = &sched, .deadline_ms = STALL_DEADLINE_MS };
    const boundary = ex.exchange();

    const t0 = common.clock.nowMillis();
    const r = boundary.post(testing.allocator, .{ .url = url, .body = "{}" });
    const elapsed = common.clock.nowMillis() - t0;
    // The stalled read is cut by the watchdog → the broker maps it to a failure.
    try testing.expectError(error.HttpExchangeFailed, r);
    // Returned from the fired deadline, not the vendor (which never answers).
    try testing.expect(elapsed < ELAPSED_BOUND_MS);
}
