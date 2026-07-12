//! Fail-closed proofs for the shared TLS pin. The defect class this module
//! exists to prevent — a direct connect on unprimed certificate state
//! dereferencing a null validation clock — hid for months because every test
//! in the tree drove plain-http loopback URLs; these tests force the secure
//! branches deterministically, with no network: a failed prime must refuse
//! the pin, the refresh branch must not rescan, and URL refusals must fire
//! before any connect.
//!
//! std-only imports: this file compiles inside the `http_pin` module, whose
//! build-graph declaration carries no named-module dependencies.

const std = @import("std");
const testing = std.testing;
const http_pin = @import("http_pin.zig");

fn testIo() std.Io {
    return std.Io.Threaded.global_single_threaded.io();
}

/// Read the kernel-assigned port off a bound listener (Zig 0.16 exposes no
/// getsockname on std.Io.net.Server, so go through libc on the raw fd).
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the != 0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// A plain-TCP listener that records whether anything ever connected. This is
/// the whole point of the regression test below: the production panic fired
/// INSIDE `Connection.Tls.create`, which std reaches only AFTER the TCP connect
/// succeeds. A refusal that is proven only against an unresolvable hostname
/// dies at DNS and never reaches the panic site, so it proves nothing about it.
/// A reachable loopback peer does — if the guard ever regresses, the connect
/// lands here and the null clock is dereferenced for real.
const AcceptProbe = struct {
    listener: *std.Io.net.Server,
    io: std.Io,
    accepted: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *AcceptProbe) void {
        const conn = self.listener.accept(self.io) catch return; // deinit wakes us
        self.accepted.store(true, .seq_cst);
        conn.close(self.io);
    }
};

test "a failed certificate rescan leaves the clock null and refuses the secure pin" {
    // fail_index 0: the bundle rescan's first allocation fails, so priming
    // cannot populate certificate state.
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    http_pin.primeTlsForDirectConnect(&client, client.io, true);
    try testing.expect(client.now == null);

    // The refusal fires on the null clock BEFORE any connect — pre-fix this
    // was the panic site (connect reading client.now.?), not a null return.
    try testing.expect(http_pin.pinPooledHandle(&client, "https://example.invalid/") == null);
    try testing.expect(http_pin.connectPinned(&client, "example.invalid", 443, true) == null);
}

test "a plain-http pin never touches certificate state" {
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    http_pin.primeTlsForDirectConnect(&client, client.io, false);
    try testing.expect(client.now == null);
    // No allocation was even attempted: the tls=false early-return is the
    // reason plain-http call sites never surfaced the panic in production.
    try testing.expect(!failing.has_induced_failure);
}

test "an already-primed client refreshes the clock without a rescan" {
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = testIo() };
    defer client.deinit();

    client.now = std.Io.Clock.real.now(client.io);
    http_pin.primeTlsForDirectConnect(&client, client.io, true);
    // The refresh branch assigns the clock directly; a rescan would have hit
    // the failing allocator. Long-lived clients must keep validating rotated
    // certificates against current time without re-reading the trust store.
    try testing.expect(client.now != null);
    try testing.expect(!failing.has_induced_failure);
}

test "unusable URLs refuse the pin before any connect" {
    var client: std.http.Client = .{ .allocator = testing.allocator, .io = testIo() };
    defer client.deinit();

    try testing.expect(http_pin.pinPooledHandle(&client, "not a url") == null);
    try testing.expect(http_pin.connectPinned(&client, "", 80, false) == null);
}

// ── Live secure-endpoint sweep ──────────────────────────────────────────────
//
// Every production host this pin actually dials: the control plane the runner
// heartbeats (the endpoint whose FIRST heartbeat panicked and crash-looped the
// runner for ~15h), the connector APIs, and the inference providers. These go
// through `connectPinned` — the exact function `control_plane_client
// .pooledHandle` calls for every runner verb, and the one that panicked.
//
// This runs in the ordinary unit suite, unconditionally. It was previously a
// sweep run BY HAND, which protected nothing: a regression only resurfaces when
// someone remembers to re-run it, and nobody does. Real certificate priming
// against real certificate chains is the property under test, so the handshake
// has to be real — a hermetic stub cannot prove the trust store loads.
//
// It is also self-policing: it immediately caught two hosts that do not exist
// (`api.agentsfleet.net`, which is not deployed, and `api.openrouter.ai`, whose
// real host is `openrouter.ai`) — bad entries a manual sweep had waved through.
const LIVE_TLS_HOSTS = [_][]const u8{
    // Control plane — the exact endpoint whose first heartbeat panicked.
    "api-dev.agentsfleet.net",
    // Connectors (bounded_fetch / serve_broker pin the same way).
    "slack.com",
    "api.github.com",
    "codeload.github.com",
    "accounts.zoho.com",
    "desk.zoho.com",
    "auth.atlassian.com",
    "api.atlassian.com",
    "api.linear.app",
    // Inference providers reached through the same armed-fetch path.
    "api.openai.com",
    "api.anthropic.com",
    "api.fireworks.ai",
    "openrouter.ai",
    "api.x.ai",
    // Platform dependencies that ride the pin.
    "api.clerk.com",
    "us.i.posthog.com",
};

const HTTPS_PORT: u16 = 443;

test "every production secure endpoint primes its certificate state and pins without panicking" {
    const io = testIo();
    var failures: usize = 0;
    for (LIVE_TLS_HOSTS) |host| {
        // A fresh client per host — exactly as the daemon's connector and broker
        // sites build one, and as the runner's client starts — so every iteration
        // exercises the COLD, unprimed state the runner booted into. This is the
        // precise call `control_plane_client.pooledHandle` makes.
        var client: std.http.Client = .{ .allocator = testing.allocator, .io = io };
        defer client.deinit();

        const handle = http_pin.connectPinned(&client, host, HTTPS_PORT, true);
        if (handle == null) {
            std.debug.print("secure-pin sweep: {s} refused the pin\n", .{host});
            failures += 1;
            continue;
        }
        // Priming populated the validation clock — the null whose dereference in
        // `Connection.Tls.create` was the production panic. A real handshake
        // completed against a real certificate chain.
        try testing.expect(client.now != null);
    }
    try testing.expectEqual(@as(usize, 0), failures);
}

test "an unprimed secure pin to a REACHABLE peer never opens the socket (the crash-loop regression)" {
    // The production crash: the runner's first control-plane heartbeat pinned a
    // pooled socket with a direct connect while `client.now` was still null, and
    // std panicked with "attempt to use null value" inside
    //   Client.connectTcp -> Connection.Tls.create -> client.now.?
    // `Tls.create` runs only AFTER the TCP connect succeeds, so the peer MUST be
    // reachable for this path to be exercised at all — a refusal proven against
    // an unresolvable host dies at DNS and never gets near it. A plain-TCP
    // loopback listener is sufficient (and needs no TLS server): if the guard
    // regresses, the connect completes, `Tls.create` runs, and the null clock is
    // dereferenced exactly as it did in production.
    const io = testIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    const port = try boundPort(listener.socket.handle);

    var probe = AcceptProbe{ .listener = &listener, .io = io };
    const accepter = try std.Thread.spawn(.{}, AcceptProbe.run, .{&probe});

    // Starve the certificate rescan so priming cannot populate the clock — the
    // exact state the runner booted into.
    var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    var client: std.http.Client = .{ .allocator = failing.allocator(), .io = io };

    const handle = http_pin.connectPinned(&client, "127.0.0.1", port, true);

    try testing.expect(handle == null); // refused
    try testing.expect(client.now == null); // and the clock really was never primed

    client.deinit();
    listener.deinit(io); // wakes the blocked accept
    accepter.join();

    // The load-bearing assertion: the guard fired BEFORE the connect, so the
    // panic site was never reached. Pre-fix this peer would have been accepted.
    try testing.expect(!probe.accepted.load(.seq_cst));
}
