// SSE streaming integration tests — covers the three live-tail invariants
// the spec calls out (publish latency, reconnect-backfill, sequence reset).
// Each test boots an in-process TestHarness, opens a real TCP SSE client
// against the harness port, PUBLISHes frames to `agent:{id}:activity`, and
// asserts the operator-visible state.
//
// Requires TEST_DATABASE_URL, TEST_REDIS_TLS_URL, and REDIS_URL_API — skipped
// gracefully otherwise. `make test-integration` exports both Redis vars
// pointing at the same instance so the handler-side subscriber and the
// test-side publisher converge on it.
//
// Shared plumbing (operator token, JWKS, workspace seed, publisher,
// close-and-wake) lives in sse_test_fixtures.zig — also consumed by
// backpressure_integration_test.zig.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const id_format = @import("../../../types/id_format.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("test_sse_client.zig");
const fixtures = @import("sse_test_fixtures.zig");

const ALLOC = std.testing.allocator;

const AGENTSFLEET_LATENCY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
const AGENTSFLEET_RECONNECT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd02";
const AGENTSFLEET_SEQUENCE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd03";

fn setupHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try fixtures.startHarnessWithWorkspace(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    inline for (.{ AGENTSFLEET_LATENCY, AGENTSFLEET_RECONNECT, AGENTSFLEET_SEQUENCE }, .{ "sse-latency", "sse-reconnect", "sse-sequence" }) |zid, name| {
        try fixtures.seedZombie(conn, zid, name);
    }
    return h;
}

/// Connect, settle SUBSCRIBE, drain any frames already buffered. Each test
/// reuses this so the timed loops measure publish-to-receive only, not
/// connection setup.
fn openAndSettle(alloc: std.mem.Allocator, port: u16, zombie_id: []const u8, opts: SseClient.ConnectOptions) !SseClient {
    const path = try fixtures.streamPath(alloc, zombie_id);
    defer alloc.free(path);
    const sc = try SseClient.connect(alloc, port, path, opts);
    common.sleepNanos(fixtures.SUBSCRIBE_SETTLE_NS);
    return sc;
}

fn percentile(values: []u64, pct: f64) u64 {
    std.mem.sort(u64, values, {}, std.sort.asc(u64));
    if (values.len == 0) return 0;
    const idx_f: f64 = pct * @as(f64, @floatFromInt(values.len - 1));
    const idx: usize = @intFromFloat(@floor(idx_f));
    return values[idx];
}

// ── test_sse_publish_latency_p95_lt_200ms ───────────────────────────────────

test "integration: SSE publish→receive latency p95 < 200ms over 50 trials" {
    const h = setupHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_LATENCY);
    defer ALLOC.free(channel);

    var sc = try openAndSettle(ALLOC, h.port, AGENTSFLEET_LATENCY, .{ .bearer = fixtures.TOKEN_OPERATOR });
    defer sc.deinit();

    // Warmup: 3 round-trips to flush slow-start latency before measuring.
    var warmup_i: usize = 0;
    while (warmup_i < 3) : (warmup_i += 1) {
        try pub_client.publish(channel, "{\"kind\":\"warmup\",\"event_id\":\"w\",\"actor\":\"steer:t\"}");
        var f = try sc.nextFrame();
        f.deinit(ALLOC);
    }

    const N = 50;
    var latencies = try ALLOC.alloc(u64, N);
    defer ALLOC.free(latencies);

    var i: usize = 0;
    while (i < N) : (i += 1) {
        const t_pub = clock.nowNanos();
        try pub_client.publish(channel, "{\"kind\":\"chunk\",\"event_id\":\"e1\",\"text\":\"x\"}");
        var f = try sc.nextFrame();
        defer f.deinit(ALLOC);
        const t_recv = clock.nowNanos();
        latencies[i] = @intCast(@divTrunc(t_recv - t_pub, std.time.ns_per_ms));
    }

    const p95_ms = percentile(latencies, 0.95);
    if (p95_ms >= 200) {
        std.debug.print("SSE p95 latency = {d}ms (budget 200ms)\n", .{p95_ms});
        return error.SsePublishLatencyP95ExceedsBudget;
    }

    fixtures.closeAndWakeSubscriber(&sc, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

// ── test_sse_reconnect_backfills_via_events ─────────────────────────────────

fn insertZombieEvent(conn: *pg.Conn, zombie_id: []const u8, event_id: []const u8, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (uid, zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'steer:test', 'chat', 'processed',
        \\        '{"message":"x"}'::jsonb, $5, $5)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{ uid, zombie_id, event_id, fixtures.TEST_WORKSPACE_ID, ts });
}

test "integration: SSE reconnect — durable backfill via /events covers the gap" {
    const h = setupHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_RECONNECT);
    defer ALLOC.free(channel);

    // Phase 1: live zombie sees event A while operator is connected.
    const ts_base: i64 = 1_900_000_001_000;
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertZombieEvent(conn, AGENTSFLEET_RECONNECT, "1900000001000-0", ts_base);
    }
    var sc1 = try openAndSettle(ALLOC, h.port, AGENTSFLEET_RECONNECT, .{ .bearer = fixtures.TOKEN_OPERATOR });
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"1900000001000-0\",\"actor\":\"steer:test\"}");
    var live_a = try sc1.nextFrame();
    live_a.deinit(ALLOC);

    // Phase 2: operator drops mid-stream. Worker durably persists B and C
    // while no SSE subscriber is connected — pub/sub frames for B/C are
    // lost (ephemeral by design); the durable rows survive.
    fixtures.closeAndWakeSubscriber(&sc1, &pub_client, channel);
    sc1.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertZombieEvent(conn, AGENTSFLEET_RECONNECT, "1900000001001-0", ts_base + 1);
        try insertZombieEvent(conn, AGENTSFLEET_RECONNECT, "1900000001002-0", ts_base + 2);
    }

    // Phase 3: operator backfills via /events?since= and observes B + C.
    const events_path = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=1h", .{ fixtures.TEST_WORKSPACE_ID, AGENTSFLEET_RECONNECT });
    defer ALLOC.free(events_path);
    const r = try (try h.get(events_path).bearer(fixtures.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("1900000001001-0"));
    try std.testing.expect(r.bodyContains("1900000001002-0"));

    // Phase 4: re-open SSE; live frame for D arrives on the new connection.
    var sc2 = try openAndSettle(ALLOC, h.port, AGENTSFLEET_RECONNECT, .{ .bearer = fixtures.TOKEN_OPERATOR });
    defer sc2.deinit();
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"1900000001003-0\",\"actor\":\"steer:test\"}");
    var live_d = try sc2.nextFrame();
    defer live_d.deinit(ALLOC);
    try std.testing.expect(std.mem.indexOf(u8, live_d.data, "1900000001003-0") != null);

    fixtures.closeAndWakeSubscriber(&sc2, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}

// ── test_sse_sequence_resets_on_reconnect ───────────────────────────────────

test "integration: SSE id resets to 0 on reconnect — Last-Event-ID ignored" {
    const h = setupHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var pub_client = fixtures.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try fixtures.activityChannel(ALLOC, AGENTSFLEET_SEQUENCE);
    defer ALLOC.free(channel);

    // First connection: receive frames id=0 and id=1.
    var sc1 = try openAndSettle(ALLOC, h.port, AGENTSFLEET_SEQUENCE, .{ .bearer = fixtures.TOKEN_OPERATOR });
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"e1\",\"actor\":\"steer:test\"}");
    var f0 = try sc1.nextFrame();
    defer f0.deinit(ALLOC);
    try std.testing.expectEqualStrings("0", f0.id);

    try pub_client.publish(channel, "{\"kind\":\"chunk\",\"event_id\":\"e1\",\"text\":\"x\"}");
    var f1 = try sc1.nextFrame();
    defer f1.deinit(ALLOC);
    try std.testing.expectEqualStrings("1", f1.id);

    fixtures.closeAndWakeSubscriber(&sc1, &pub_client, channel);
    sc1.deinit();

    // Reconnect with a forged Last-Event-ID; the handler must ignore it.
    var sc2 = try openAndSettle(ALLOC, h.port, AGENTSFLEET_SEQUENCE, .{
        .bearer = fixtures.TOKEN_OPERATOR,
        .last_event_id = "99",
    });
    defer sc2.deinit();
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"e2\",\"actor\":\"steer:test\"}");
    var first = try sc2.nextFrame();
    defer first.deinit(ALLOC);
    try std.testing.expectEqualStrings("0", first.id);

    fixtures.closeAndWakeSubscriber(&sc2, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.cleanupWorkspaceData(conn);
}
