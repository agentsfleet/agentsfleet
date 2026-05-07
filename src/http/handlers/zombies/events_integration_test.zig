// HTTP integration tests for the events read endpoints:
//   GET /v1/workspaces/{ws}/zombies/{id}/events
//   GET /v1/workspaces/{ws}/events
//   GET /v1/workspaces/{ws}/zombies/{id}/events/stream  (auth surface only)
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. SSE happy-path
// (Bearer 200 + SUBSCRIBE) is exercised in tests that need Redis; the auth
// surface (no-bearer / invalid-bearer 401) runs here without Redis.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

// Reuse the JWT signing fixture (kid + JWKS + tenant/workspace claims) from
// steer_integration_test.zig so we don't have to mint a fresh signature.
// Zombie ids must NOT collide with cross_workspace_idor_test (…bbb01) or
// event_loop_execution_tracking_test (…bbb01) — those suites' `ON CONFLICT
// DO NOTHING` seeds preserve whichever workspace_id stamped the row first,
// so a shared id leaks ownership across suites and breaks the loser.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const ZOMBIE_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0eee01";
const ZOMBIE_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0eee02";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
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
        \\VALUES ($1, 'EventsTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'events-a', '---\nname: events-a\n---\ntest', '{"name":"events-a"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_A, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'events-b', '---\nname: events-b\n---\ntest', '{"name":"events-b"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_B, TEST_WORKSPACE_ID });

    // Seed a mix of actors across both zombies. Distinct created_at for
    // deterministic ordering. event_id mirrors the Redis stream entry shape.
    try insertEvent(conn, ZOMBIE_A, "1700000000000-0", "steer:kishore", "chat", 1_700_000_000_000);
    try insertEvent(conn, ZOMBIE_A, "1700000000001-0", "steer:kishore", "chat", 1_700_000_000_001);
    try insertEvent(conn, ZOMBIE_A, "1700000000002-0", "webhook:github", "webhook", 1_700_000_000_002);
    try insertEvent(conn, ZOMBIE_B, "1700000000003-0", "steer:kishore", "chat", 1_700_000_000_003);
    try insertEvent(conn, ZOMBIE_B, "1700000000004-0", "cron:0_*/30", "cron", 1_700_000_000_004);
}

fn insertEvent(conn: *pg.Conn, zombie_id: []const u8, event_id: []const u8, actor: []const u8, event_type: []const u8, ts: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, 'processed',
        \\        '{"message":"test"}'::jsonb, $6, $6)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{ zombie_id, event_id, TEST_WORKSPACE_ID, actor, event_type, ts });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
}

// ── Auth + path-shape (no Redis needed) ─────────────────────────────────────

test "integration: events GET — no bearer → 401" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (h.get(url)).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

test "integration: events GET — since and cursor mutually exclusive → 400" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=2h&cursor=abc", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try std.testing.expect(r.bodyContains("since_and_cursor_mutually_exclusive"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

test "integration: events GET — invalid since format → 400" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=bogus", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try std.testing.expect(r.bodyContains("invalid_since_format"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── SSE auth (no Redis) ─────────────────────────────────────────────────────

// test_sse_auth_missing_authorization_401
test "integration: events stream — no bearer → 401" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events/stream", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (h.get(url)).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// test_sse_auth_invalid_bearer_401
test "integration: events stream — invalid bearer → 401" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events/stream", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (try (h.get(url)).bearer("not.a.real.jwt")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── Filters: actor glob, since-param, workspace aggregation ────────────────

// test_actor_filter_steer
test "integration: events GET — actor=steer:* glob filter returns steer events only" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Seed has 3 steer:kishore events (2 in ZOMBIE_A, 1 in ZOMBIE_B), 1 webhook,
    // 1 cron — across the workspace. The glob `steer:*` becomes SQL `steer:%`.
    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/events?actor=steer:*", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("steer:kishore"));
    try std.testing.expect(!r.bodyContains("webhook:github"));
    try std.testing.expect(!r.bodyContains("cron:0_*/30"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// test_actor_filter_webhook_github
test "integration: events GET — actor=webhook:github exact filter" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/events?actor=webhook:github", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("webhook:github"));
    try std.testing.expect(!r.bodyContains("steer:kishore"));
    try std.testing.expect(!r.bodyContains("cron:0_*/30"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// test_workspace_events_endpoint
test "integration: workspace events GET — sorted DESC + zombie_id drill-down filter" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // All five seeded events come back unfiltered.
    {
        const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/events", .{TEST_WORKSPACE_ID});
        defer ALLOC.free(url);
        const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        // Newest first: created_at=…004 (cron) precedes …000 (oldest) in body order.
        const idx_004 = std.mem.indexOf(u8, r.body, "1700000000004-0") orelse return error.TestFailed;
        const idx_000 = std.mem.indexOf(u8, r.body, "1700000000000-0") orelse return error.TestFailed;
        try std.testing.expect(idx_004 < idx_000);
    }

    // ?zombie_id=ZOMBIE_A drills down to that zombie's three events.
    {
        const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/events?zombie_id={s}", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
        defer ALLOC.free(url);
        const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("1700000000000-0"));
        try std.testing.expect(r.bodyContains("1700000000002-0"));
        try std.testing.expect(!r.bodyContains("1700000000003-0")); // ZOMBIE_B event
        try std.testing.expect(!r.bodyContains("1700000000004-0")); // ZOMBIE_B event
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// test_since_param_duration
test "integration: events GET — since=2h filters by relative duration" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Insert one recent event (now - 1h, kept) and one stale event (now - 5h,
    // filtered). Seed events at 1.7e12 ms are far older than now and excluded.
    const now_ms = std.time.milliTimestamp();
    const recent_ts = now_ms - (60 * 60 * 1000);
    const stale_ts = now_ms - (5 * 60 * 60 * 1000);
    const recent_eid = "1900000000001-0"; // distinct prefix to avoid seed clash
    const stale_eid = "1900000000002-0";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertEvent(conn, ZOMBIE_A, recent_eid, "steer:kishore", "chat", recent_ts);
        try insertEvent(conn, ZOMBIE_A, stale_eid, "steer:kishore", "chat", stale_ts);
    }

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=2h", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains(recent_eid));
    try std.testing.expect(!r.bodyContains(stale_eid));
    try std.testing.expect(!r.bodyContains("1700000000000-0")); // 2023-era seed

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

fn insertEventWithParent(
    conn: *pg.Conn,
    zombie_id: []const u8,
    event_id: []const u8,
    actor: []const u8,
    status: []const u8,
    resumes_event_id: ?[]const u8,
    ts: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, resumes_event_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, 'chat', $5,
        \\        '{"message":"test"}'::jsonb, $6, $7, $7)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{ zombie_id, event_id, TEST_WORKSPACE_ID, actor, status, resumes_event_id, ts });
}

// test_resumes_event_id_immediate_parent
test "integration: resumes_event_id walks the chain via recursive CTE" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Chain: A (origin) → B (continuation chunk) → C (gate-blocked) → D (resumed).
    // Each event's resumes_event_id points at the *immediate* parent — never a
    // grandparent — so the recursive walk reproduces the chain top-to-bottom.
    const a_eid = "1900000000020-0";
    const b_eid = "1900000000021-0";
    const c_eid = "1900000000022-0";
    const d_eid = "1900000000023-0";
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try insertEventWithParent(conn, ZOMBIE_A, a_eid, "steer:kishore", "processed", null, 1_900_000_000_020);
    try insertEventWithParent(conn, ZOMBIE_A, b_eid, "continuation:steer:kishore", "processed", a_eid, 1_900_000_000_021);
    try insertEventWithParent(conn, ZOMBIE_A, c_eid, "continuation:steer:kishore", "gate_blocked", b_eid, 1_900_000_000_022);
    try insertEventWithParent(conn, ZOMBIE_A, d_eid, "continuation:steer:kishore", "processed", c_eid, 1_900_000_000_023);

    // Walk from D back to A. Depth 1 = D, depth 4 = A.
    const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
    var q = PgQuery.from(try conn.query(
        \\WITH RECURSIVE chain AS (
        \\    SELECT event_id, resumes_event_id, 1 AS depth
        \\    FROM core.zombie_events
        \\    WHERE zombie_id = $1::uuid AND event_id = $2
        \\  UNION ALL
        \\    SELECT e.event_id, e.resumes_event_id, c.depth + 1
        \\    FROM core.zombie_events e
        \\    JOIN chain c ON e.event_id = c.resumes_event_id
        \\    WHERE e.zombie_id = $1::uuid
        \\)
        \\SELECT event_id, depth FROM chain ORDER BY depth ASC
    , .{ ZOMBIE_A, d_eid }));
    defer q.deinit();

    const expected_order = [_][]const u8{ d_eid, c_eid, b_eid, a_eid };
    var i: usize = 0;
    while (try q.next()) |row| : (i += 1) {
        if (i >= expected_order.len) return error.UnexpectedExtraRow;
        try std.testing.expectEqualStrings(expected_order[i], try row.get([]const u8, 0));
        try std.testing.expectEqual(@as(i32, @intCast(i + 1)), try row.get(i32, 1));
    }
    try std.testing.expectEqual(expected_order.len, i);

    cleanupTestData(conn);
}

// test_since_param_rfc3339
test "integration: events GET — since=ISO8601 absolute timestamp" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // 2025-04-25T00:00:00Z = 1745539200000 ms. Insert one event before, one after.
    const cutoff_ms: i64 = 1_745_539_200_000;
    const before_eid = "1900000000010-0";
    const after_eid = "1900000000011-0";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try insertEvent(conn, ZOMBIE_A, before_eid, "steer:kishore", "chat", cutoff_ms - 1);
        try insertEvent(conn, ZOMBIE_A, after_eid, "steer:kishore", "chat", cutoff_ms + 1);
    }

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events?since=2025-04-25T00:00:00Z", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains(after_eid));
    try std.testing.expect(!r.bodyContains(before_eid));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
