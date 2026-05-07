// SSE streaming integration tests — covers the three live-tail invariants
// the spec calls out (publish latency, reconnect-backfill, sequence reset).
// Each test boots an in-process TestHarness, opens a real TCP SSE client
// against the harness port, PUBLISHes frames to `zombie:{id}:activity`, and
// asserts the operator-visible state.
//
// Requires TEST_DATABASE_URL, TEST_REDIS_TLS_URL, and REDIS_URL_API — skipped
// gracefully otherwise. `make test-integration` exports both Redis vars
// pointing at the same instance so the handler-side subscriber and the
// test-side publisher converge on it.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const queue_redis = @import("../../../queue/redis_client.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("test_sse_client.zig");

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const ZOMBIE_LATENCY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
const ZOMBIE_RECONNECT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd02";
const ZOMBIE_SEQUENCE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd03";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

const SUBSCRIBE_SETTLE_NS: u64 = 200 * std.time.ns_per_ms;
const TEST_REDIS_URL_ENV = "TEST_REDIS_TLS_URL";
const HANDLER_REDIS_URL_ENV = "REDIS_URL_API";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

/// Skip unless both env vars resolve to a usable Redis: TEST_REDIS_TLS_URL
/// for the test-side publisher and REDIS_URL_API for the SSE handler's
/// subscriber. `make test-integration` exports both pointing at the same
/// instance.
fn requireRedisEnvOrSkip(alloc: std.mem.Allocator) !void {
    const handler = std.process.getEnvVarOwned(alloc, HANDLER_REDIS_URL_ENV) catch return error.SkipZigTest;
    alloc.free(handler);
    const tester = std.process.getEnvVarOwned(alloc, TEST_REDIS_URL_ENV) catch return error.SkipZigTest;
    alloc.free(tester);
}

fn setupHarness(alloc: std.mem.Allocator) !*TestHarness {
    try requireRedisEnvOrSkip(alloc);

    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'SseStreamingTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    inline for (.{ ZOMBIE_LATENCY, ZOMBIE_RECONNECT, ZOMBIE_SEQUENCE }, .{ "sse-latency", "sse-reconnect", "sse-sequence" }) |zid, name| {
        _ = try conn.exec(
            \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
            \\VALUES ($1, $2, $3, '---\nname: zz\n---\ntest', '{"name":"zz"}', 'active', 0, 0)
            \\ON CONFLICT DO NOTHING
        , .{ zid, TEST_WORKSPACE_ID, name });
    }
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
}

fn connectPublisher(alloc: std.mem.Allocator) !queue_redis.Client {
    const tls_url = try std.process.getEnvVarOwned(alloc, TEST_REDIS_URL_ENV);
    defer alloc.free(tls_url);
    return queue_redis.Client.connectFromUrl(alloc, tls_url);
}

fn streamPath(alloc: std.mem.Allocator, zombie_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/events/stream", .{ TEST_WORKSPACE_ID, zombie_id });
}

fn activityChannel(alloc: std.mem.Allocator, zombie_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "zombie:{s}:activity", .{zombie_id});
}

/// Connect, settle SUBSCRIBE, drain any frames already buffered. Each test
/// reuses this so the timed loops measure publish-to-receive only, not
/// connection setup.
fn openAndSettle(alloc: std.mem.Allocator, port: u16, zombie_id: []const u8, opts: SseClient.ConnectOptions) !SseClient {
    const path = try streamPath(alloc, zombie_id);
    defer alloc.free(path);
    const sc = try SseClient.connect(alloc, port, path, opts);
    std.Thread.sleep(SUBSCRIBE_SETTLE_NS);
    return sc;
}

/// Close the client socket, then PUBLISH one sentinel frame so the handler's
/// streamLoop wakes from `subscriber.nextMessage()`, attempts a write, hits
/// BrokenPipe, and releases its httpz worker thread. Without this the worker
/// stays wedged on the Redis read until the next channel publish — which
/// for a single-test channel never arrives, starving the 2-worker pool
/// before `h.deinit()` runs.
fn closeAndWakeSubscriber(sc: *SseClient, pub_client: *queue_redis.Client, channel: []const u8) void {
    sc.closeStream();
    pub_client.publish(channel, "{\"kind\":\"drain\",\"event_id\":\"_\"}") catch {};
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

    var pub_client = connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try activityChannel(ALLOC, ZOMBIE_LATENCY);
    defer ALLOC.free(channel);

    var sc = try openAndSettle(ALLOC, h.port, ZOMBIE_LATENCY, .{ .bearer = TOKEN_OPERATOR });
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
        const t_pub = std.time.nanoTimestamp();
        try pub_client.publish(channel, "{\"kind\":\"chunk\",\"event_id\":\"e1\",\"text\":\"x\"}");
        var f = try sc.nextFrame();
        defer f.deinit(ALLOC);
        const t_recv = std.time.nanoTimestamp();
        latencies[i] = @intCast(@divTrunc(t_recv - t_pub, std.time.ns_per_ms));
    }

    const p95_ms = percentile(latencies, 0.95);
    if (p95_ms >= 200) {
        std.debug.print("SSE p95 latency = {d}ms (budget 200ms)\n", .{p95_ms});
        return error.SsePublishLatencyP95ExceedsBudget;
    }

    closeAndWakeSubscriber(&sc, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── test_sse_reconnect_backfills_via_events ─────────────────────────────────

fn insertZombieEvent(conn: *pg.Conn, zombie_id: []const u8, event_id: []const u8, ts: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, 'steer:test', 'chat', 'processed',
        \\        '{"message":"x"}'::jsonb, $4, $4)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{ zombie_id, event_id, TEST_WORKSPACE_ID, ts });
}

test "integration: SSE reconnect — durable backfill via /events covers the gap" {
    const h = setupHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var pub_client = connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try activityChannel(ALLOC, ZOMBIE_RECONNECT);
    defer ALLOC.free(channel);

    // Phase 1: live zombie sees event A while operator is connected.
    const ts_base: i64 = 1_900_000_001_000;
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertZombieEvent(conn, ZOMBIE_RECONNECT, "1900000001000-0", ts_base);
    }
    var sc1 = try openAndSettle(ALLOC, h.port, ZOMBIE_RECONNECT, .{ .bearer = TOKEN_OPERATOR });
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"1900000001000-0\",\"actor\":\"steer:test\"}");
    var live_a = try sc1.nextFrame();
    live_a.deinit(ALLOC);

    // Phase 2: operator drops mid-stream. Worker durably persists B and C
    // while no SSE subscriber is connected — pub/sub frames for B/C are
    // lost (ephemeral by design); the durable rows survive.
    closeAndWakeSubscriber(&sc1, &pub_client, channel);
    sc1.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertZombieEvent(conn, ZOMBIE_RECONNECT, "1900000001001-0", ts_base + 1);
        try insertZombieEvent(conn, ZOMBIE_RECONNECT, "1900000001002-0", ts_base + 2);
    }

    // Phase 3: operator backfills via /events?since= and observes B + C.
    const events_path = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=1h", .{ TEST_WORKSPACE_ID, ZOMBIE_RECONNECT });
    defer ALLOC.free(events_path);
    const r = try (try h.get(events_path).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("1900000001001-0"));
    try std.testing.expect(r.bodyContains("1900000001002-0"));

    // Phase 4: re-open SSE; live frame for D arrives on the new connection.
    var sc2 = try openAndSettle(ALLOC, h.port, ZOMBIE_RECONNECT, .{ .bearer = TOKEN_OPERATOR });
    defer sc2.deinit();
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"1900000001003-0\",\"actor\":\"steer:test\"}");
    var live_d = try sc2.nextFrame();
    defer live_d.deinit(ALLOC);
    try std.testing.expect(std.mem.indexOf(u8, live_d.data, "1900000001003-0") != null);

    closeAndWakeSubscriber(&sc2, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── test_sse_sequence_resets_on_reconnect ───────────────────────────────────

test "integration: SSE id resets to 0 on reconnect — Last-Event-ID ignored" {
    const h = setupHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var pub_client = connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const channel = try activityChannel(ALLOC, ZOMBIE_SEQUENCE);
    defer ALLOC.free(channel);

    // First connection: receive frames id=0 and id=1.
    var sc1 = try openAndSettle(ALLOC, h.port, ZOMBIE_SEQUENCE, .{ .bearer = TOKEN_OPERATOR });
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"e1\",\"actor\":\"steer:test\"}");
    var f0 = try sc1.nextFrame();
    defer f0.deinit(ALLOC);
    try std.testing.expectEqualStrings("0", f0.id);

    try pub_client.publish(channel, "{\"kind\":\"chunk\",\"event_id\":\"e1\",\"text\":\"x\"}");
    var f1 = try sc1.nextFrame();
    defer f1.deinit(ALLOC);
    try std.testing.expectEqualStrings("1", f1.id);

    closeAndWakeSubscriber(&sc1, &pub_client, channel);
    sc1.deinit();

    // Reconnect with a forged Last-Event-ID; the handler must ignore it.
    var sc2 = try openAndSettle(ALLOC, h.port, ZOMBIE_SEQUENCE, .{
        .bearer = TOKEN_OPERATOR,
        .last_event_id = "99",
    });
    defer sc2.deinit();
    try pub_client.publish(channel, "{\"kind\":\"event_received\",\"event_id\":\"e2\",\"actor\":\"steer:test\"}");
    var first = try sc2.nextFrame();
    defer first.deinit(ALLOC);
    try std.testing.expectEqualStrings("0", first.id);

    closeAndWakeSubscriber(&sc2, &pub_client, channel);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
