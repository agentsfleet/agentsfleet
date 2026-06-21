// Backpressure integration tests — "HTTP backpressure made real" +
// route-class admission.
//
// Dispatch leg: api-class requests above the in-flight ceiling shed 429
// with Retry-After + X-RateLimit-* headers, the rejection counter moves,
// the shed path releases its slot — while ops-class routes (/healthz,
// /readyz, /metrics) are NEVER shed and unmatched paths 404 without
// consuming admission.
// SSE leg: streams above the dedicated cap shed 503 while /healthz keeps
// answering on the same pool, a closed stream releases its slot, and the
// stream class is exempt from the api ceiling.
//
// The harness server runs in-process, so the metrics globals asserted here
// are the same ones the handlers increment. Requires TEST_DATABASE_URL;
// the SSE legs additionally require REDIS_URL_API + TEST_REDIS_TLS_URL —
// skipped gracefully otherwise (same gating as the SSE streaming suite).

const std = @import("std");
const common = @import("common");
const metrics = @import("../../../observability/metrics.zig");
const ec = @import("../../../errors/error_registry.zig");
const model_caps = @import("../model_caps.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("test_sse_client.zig");
const fixtures = @import("sse_test_fixtures.zig");

const ALLOC = std.testing.allocator;

const AGENTSFLEET_BACKPRESSURE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb001";
const AGENTSFLEET_STREAM_CLASS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb002";
const AGENTSFLEET_DRAIN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb003";
/// Deliberately never seeded — drives the 404-after-registration path.
const AGENTSFLEET_UNSEEDED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb004";
const AGENTSFLEET_FD_CYCLE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb005";
/// Open/close cycles for the fd-return proof — enough that a one-fd-per-stream
/// leak separates clearly from the baseline.
const FD_CYCLES: usize = 3;
/// Probe range for the fd-liveness count — covers every descriptor the
/// harness (pool, server, clients) plausibly holds.
const FD_PROBE_MAX: std.c.fd_t = 1024;
/// api-class, none-auth probe route — sheds at the ceiling where the
/// ops-class probes below must not.
const API_PROBE_PATH = model_caps.MODEL_CAPS_PATH;

/// Bounded poll for the parked stream's slot release after close: the handler
/// wakes on the drain publish, fails its write, unwinds, and decrements.
const SLOT_RELEASE_MAX_ATTEMPTS: usize = 40;
const SLOT_RELEASE_POLL_NS: u64 = 100 * std.time.ns_per_ms;

// ── raw-header probe ────────────────────────────────────────────────────────
// std.http.Client.fetch exposes only the status (FetchResult), so the header
// assertions read the raw response head off a plain TCP socket.

fn fetchRawHead(alloc: std.mem.Allocator, port: u16, path: []const u8) ![]u8 {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", port);
    var stream = try addr.connect(io, .{ .mode = .stream });
    defer stream.close(io);

    var req_buf: [256]u8 = undefined;
    const req = try std.fmt.bufPrint(&req_buf, "GET {s} HTTP/1.1\r\nHost: 127.0.0.1:{d}\r\nConnection: close\r\n\r\n", .{ path, port });
    var wbuf: [256]u8 = undefined;
    var w = stream.writer(io, &wbuf);
    try w.interface.writeAll(req);
    try w.interface.flush();

    var head: std.ArrayList(u8) = .empty;
    errdefer head.deinit(alloc);
    var tmp: [2048]u8 = undefined;
    while (std.mem.indexOf(u8, head.items, "\r\n\r\n") == null) {
        const n = try std.posix.read(stream.socket.handle, &tmp);
        if (n == 0) break; // server closed after writing the response
        try head.appendSlice(alloc, tmp[0..n]);
    }
    return head.toOwnedSlice(alloc);
}

fn headContains(head: []const u8, needle: []const u8) bool {
    return std.mem.indexOf(u8, head, needle) != null;
}

// ── dispatch in-flight ceiling ──────────────────────────────────────────────

test "integration: api-class requests shed 429 at the ceiling; ops routes and 404s never shed" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = fixtures.noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const before = metrics.snapshot().api_backpressure_rejections_total;

    // Ceiling 0 saturates the guard deterministically: the first api-class
    // request's own slot claim exceeds it. (The env path forbids 0 —
    // InvalidApiMaxInFlightRequests — so this state is reachable only by
    // test override.)
    h.ctx.api_max_in_flight_requests = 0;

    const shed = try (h.get(API_PROBE_PATH)).send();
    defer shed.deinit();
    try shed.expectStatus(.too_many_requests);
    try shed.expectErrorCode(ec.ERR_API_BACKPRESSURE);

    // ops class: the routes an operator needs DURING the overload answer
    // while the api surface sheds. (readyz's body depends on backing-store
    // readiness — the claim under test is only that admission never sheds it.)
    const health = try (h.get("/healthz")).send();
    defer health.deinit();
    try health.expectStatus(.ok);
    const ready = try (h.get("/readyz")).send();
    defer ready.deinit();
    try std.testing.expect(ready.status != @intFromEnum(std.http.Status.too_many_requests));
    const ops_scrape = try (h.get("/metrics")).send();
    defer ops_scrape.deinit();
    try ops_scrape.expectStatus(.ok);

    // Unmatched paths 404 without consuming admission — a 404 is cheaper
    // than the gate, and it must not count as a rejection either.
    const nope = try (h.get("/v1/no-such-route")).send();
    defer nope.deinit();
    try nope.expectStatus(.not_found);

    // Header set per REST guidelines §4 — read off a raw socket since the
    // fluent client exposes only the status.
    const head = try fetchRawHead(ALLOC, h.port, API_PROBE_PATH);
    defer ALLOC.free(head);
    try std.testing.expect(headContains(head, "429"));
    try std.testing.expect(headContains(head, "Retry-After: 1"));
    try std.testing.expect(headContains(head, "X-RateLimit-Remaining: 0"));
    try std.testing.expect(headContains(head, "X-RateLimit-Limit: 0"));
    try std.testing.expect(headContains(head, "X-RateLimit-Reset: "));

    // Exactly the two api-class sheds counted — the ops probes and the 404
    // moved nothing.
    const after = metrics.snapshot().api_backpressure_rejections_total;
    try std.testing.expectEqual(before + 2, after);

    // Ceiling 1: an api request is admitted again only if every shed/served
    // request released its slot — a leaked claim would keep live at >=1 and
    // re-shed this probe.
    h.ctx.api_max_in_flight_requests = 1;
    const ok = try (h.get(API_PROBE_PATH)).send();
    defer ok.deinit();
    try ok.expectStatus(.ok);

    // The gauge counts api-class only: the /metrics scrape itself is
    // ops-class, so at render time nothing is in flight — which is itself
    // the ops-exemption assertion.
    h.ctx.api_max_in_flight_requests = 64;
    const scrape = try (h.get("/metrics")).send();
    defer scrape.deinit();
    try scrape.expectStatus(.ok);
    try std.testing.expect(scrape.bodyContains("fleet_api_in_flight_requests 0"));
}

test "integration: registry drain closes live streams and rejects new ones" {
    const h = fixtures.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures.seedFleet(conn, AGENTSFLEET_DRAIN, "bp-drain");
    }

    const path = try fixtures.streamPath(ALLOC, AGENTSFLEET_DRAIN);
    defer ALLOC.free(path);

    var sc = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR });
    common.sleepNanos(fixtures.SUBSCRIBE_SETTLE_NS);
    try std.testing.expectEqual(@as(usize, 1), h.streams.count());

    // The shutdown choreography: drain marks the registry draining and
    // shuts the client socket (that alone wakes only write-BLOCKED threads —
    // a stream parked in its subscription pop is a futex wait, not a socket
    // read), the hub's close broadcast wakes the parked thread, and
    // awaitEmpty bounds the wait for its deregistration.
    h.streams.drain();

    // Draining registry sheds new streams with the cap's 503 immediately —
    // before the live stream has even finished tearing down.
    const denied = SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR });
    try std.testing.expectError(error.SseUnexpectedStatus, denied);

    h.hub.stop();
    h.streams.awaitEmpty();
    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
    try std.testing.expectEqual(@as(u64, 0), metrics.snapshot().sse_in_flight_streams);

    sc.closeStream();
    sc.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

test "integration: the SSE stream class is exempt from the api ceiling" {
    const h = fixtures.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures.seedFleet(conn, AGENTSFLEET_STREAM_CLASS, "bp-class");
    }

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_STREAM_CLASS);
    defer ALLOC.free(channel);
    const path = try fixtures.streamPath(ALLOC, AGENTSFLEET_STREAM_CLASS);
    defer ALLOC.free(path);

    // api saturated; the stream must still be admitted (its gate is the
    // SSE cap, not the api ceiling).
    h.ctx.api_max_in_flight_requests = 0;
    defer h.ctx.api_max_in_flight_requests = 64;

    var sc = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR });
    common.sleepNanos(fixtures.SUBSCRIBE_SETTLE_NS);

    // ...while an api-class probe sheds at the same instant.
    const shed = try (h.get(API_PROBE_PATH)).send();
    defer shed.deinit();
    try shed.expectStatus(.too_many_requests);

    fixtures.closeAndWakeSubscriber(&sc, &pub_client, channel);
    sc.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

test "integration: a stream rejected after registration releases its slot" {
    // Pins the non-handoff defer in the stream handler: a request that claims
    // a registry slot but fails authorization (unknown fleet → 404) must
    // release the slot on the request path. A leak here silently erodes the
    // stream cap one failed request at a time.
    const h = fixtures.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try fixtures.streamPath(ALLOC, AGENTSFLEET_UNSEEDED);
    defer ALLOC.free(path);

    const denied = try (try h.get(path).bearer(fixtures.TOKEN_OPERATOR)).send();
    defer denied.deinit();
    try denied.expectStatus(.not_found);
    try denied.expectErrorCode(ec.ERR_AGENTSFLEET_NOT_FOUND);

    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
    try std.testing.expectEqual(@as(u64, 0), metrics.snapshot().sse_in_flight_streams);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

fn countOpenFds() usize {
    // fcntl(F_GETFD) probes descriptor liveness without opening, closing, or
    // perturbing anything — portable across macOS and Linux. The fixed probe
    // range comfortably covers the harness's pool/server/test descriptors.
    var n: usize = 0;
    var fd: std.c.fd_t = 0;
    while (fd < FD_PROBE_MAX) : (fd += 1) {
        if (std.c.fcntl(fd, std.c.F.GETFD) != -1) n += 1;
    }
    return n;
}

test "integration: finished streams return their socket fds to the OS" {
    // Regression pin for the fd leak the registry work fixed: the disowned
    // client socket is the stream thread's to close — before the fix every
    // finished stream leaked one fd, invisible to every other suite.
    const h = fixtures.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures.seedFleet(conn, AGENTSFLEET_FD_CYCLE, "bp-fd");
    }

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_FD_CYCLE);
    defer ALLOC.free(channel);
    const path = try fixtures.streamPath(ALLOC, AGENTSFLEET_FD_CYCLE);
    defer ALLOC.free(path);

    // Warmup cycle: let lazily-created descriptors (pool checkout, caches)
    // exist before the baseline is taken.
    try runStreamCycle(h, &pub_client, channel, path);
    const baseline = countOpenFds();

    var n: usize = 0;
    while (n < FD_CYCLES) : (n += 1) {
        try runStreamCycle(h, &pub_client, channel, path);
    }

    // The slot frees before the thread's final socket close (teardown is
    // destroy → deregister → close), so poll the fd count itself.
    var attempt: usize = 0;
    var fds_now = countOpenFds();
    while (fds_now > baseline and attempt < SLOT_RELEASE_MAX_ATTEMPTS) : (attempt += 1) {
        common.sleepNanos(SLOT_RELEASE_POLL_NS);
        fds_now = countOpenFds();
    }
    try std.testing.expectEqual(baseline, fds_now);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

/// One full stream lifecycle: connect, settle, close + wake, then wait for
/// the slot release so cycles never overlap.
fn runStreamCycle(h: *TestHarness, pub_client: anytype, channel: []const u8, path: []const u8) !void {
    var sc = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR });
    common.sleepNanos(fixtures.SUBSCRIBE_SETTLE_NS);
    fixtures.closeAndWakeSubscriber(&sc, pub_client, channel);
    sc.deinit();
    var attempt: usize = 0;
    while (h.streams.count() > 0 and attempt < SLOT_RELEASE_MAX_ATTEMPTS) : (attempt += 1) {
        common.sleepNanos(SLOT_RELEASE_POLL_NS);
    }
    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
}

// ── SSE stream cap ──────────────────────────────────────────────────────────

test "integration: SSE streams above the cap shed 503 while healthz stays alive" {
    const h = fixtures.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures.seedFleet(conn, AGENTSFLEET_BACKPRESSURE, "bp-cap");
    }

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_BACKPRESSURE);
    defer ALLOC.free(channel);
    const path = try fixtures.streamPath(ALLOC, AGENTSFLEET_BACKPRESSURE);
    defer ALLOC.free(path);

    h.ctx.sse_max_streams = 1;

    // Stream 1 occupies the only slot and parks its handler thread.
    var sc1 = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR });
    common.sleepNanos(fixtures.SUBSCRIBE_SETTLE_NS);

    const before = metrics.snapshot().sse_backpressure_rejections_total;

    // Stream 2 sheds 503 — and completes as a normal response, so the
    // fluent client can assert on it.
    const shed = try (try h.get(path).bearer(fixtures.TOKEN_OPERATOR)).send();
    defer shed.deinit();
    try shed.expectStatus(.service_unavailable);
    try shed.expectErrorCode(ec.ERR_SSE_STREAM_CAP);

    const after = metrics.snapshot().sse_backpressure_rejections_total;
    try std.testing.expectEqual(before + 1, after);

    // The invariant the cap exists for: a saturated stream cap leaves
    // handler threads free — /healthz answers while the stream is parked.
    const health = try (h.get("/healthz")).send();
    defer health.deinit();
    try health.expectStatus(.ok);

    // Gauge reports the parked stream.
    const scrape = try (h.get("/metrics")).send();
    defer scrape.deinit();
    try scrape.expectStatus(.ok);
    try std.testing.expect(scrape.bodyContains("fleet_sse_in_flight_streams 1"));

    // Closing the parked stream releases its slot: a fresh stream is
    // admitted within the poll budget. Each failed attempt sheds 503 and
    // releases its own claim, so the poll cannot wedge the cap.
    fixtures.closeAndWakeSubscriber(&sc1, &pub_client, channel);
    sc1.deinit();

    var reopened: ?SseClient = null;
    var attempt: usize = 0;
    while (attempt < SLOT_RELEASE_MAX_ATTEMPTS) : (attempt += 1) {
        const sc = SseClient.connect(ALLOC, h.port, path, .{ .bearer = fixtures.TOKEN_OPERATOR }) catch |err| switch (err) {
            error.SseUnexpectedStatus => {
                common.sleepNanos(SLOT_RELEASE_POLL_NS);
                continue;
            },
            else => return err,
        };
        reopened = sc;
        break;
    }
    try std.testing.expect(reopened != null);
    var sc2 = reopened.?;
    fixtures.closeAndWakeSubscriber(&sc2, &pub_client, channel);
    sc2.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}
