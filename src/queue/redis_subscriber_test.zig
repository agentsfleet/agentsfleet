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
    try std.testing.expect(elapsed_ns >= 50 * std.time.ns_per_ms);
    try std.testing.expect(elapsed_ns < 5 * std.time.ns_per_s);
}
