// HTTP integration tests for zombie steer endpoint (M23_001).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. Success-path
// tests (idle + active) additionally require a reachable Redis (they use
// the steer-message queue) and self-skip via `h.tryConnectRedis()`.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../auth/middleware/mod.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aff01";
const ZOMBIE_IDLE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa01";
const ZOMBIE_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa02";
const ZOMBIE_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa03";
const SESSION_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa10";
const ACTIVE_EXEC_ID = "test-exec-steer-001";
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
    _ = h.tryConnectRedis(); // optional — success-path tests gate on has_redis
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'SteerTest', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, std.time.milliTimestamp() });
    const now = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/steer', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/other', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-idle', '---\nname: steer-idle\n---\ntest', '{"name":"steer-idle"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_IDLE, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-active', '---\nname: steer-active\n---\ntest', '{"name":"steer-active"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_ACTIVE, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombie_sessions (id, zombie_id, context_json, execution_id, execution_started_at, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, '{}', $3, 1000, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE SET execution_id=EXCLUDED.execution_id, execution_started_at=EXCLUDED.execution_started_at
    , .{ SESSION_ACTIVE, ZOMBIE_ACTIVE, ACTIVE_EXEC_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-otherws', '---\nname: steer-otherws\n---\ntest', '{"name":"steer-otherws"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_sessions WHERE zombie_id IN ($1, $2, $3)", .{ ZOMBIE_IDLE, ZOMBIE_ACTIVE, ZOMBIE_OTHER_WS }) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id IN ($1, $2)", .{ TEST_WORKSPACE_ID, OTHER_WS_ID }) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch {};
}

// ── Auth + body validation (no Redis needed) ────────────────────────────────

test "integration: zombie steer — auth and body validation" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const steer_idle = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/steer", .{ TEST_WORKSPACE_ID, ZOMBIE_IDLE });
    defer ALLOC.free(steer_idle);
    // steer_other: caller's workspace in URL path, but zombie lives in OTHER_WS — handler 404.
    const steer_other = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/steer", .{ TEST_WORKSPACE_ID, ZOMBIE_OTHER_WS });
    defer ALLOC.free(steer_other);
    const body_valid = "{\"message\":\"redirect to phase 2\"}";
    const body_empty = "{\"message\":\"\"}";
    const body_toolong = "{\"message\":\"" ++ "x" ** 8193 ++ "\"}";

    { // no bearer → 401
        const r = try (try h.post(steer_idle).json(body_valid)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    { // zombie in different workspace → 404
        const r = try (try (try h.post(steer_other).bearer(TOKEN_OPERATOR)).json(body_valid)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
    }
    { // empty message → 400
        const r = try (try (try h.post(steer_idle).bearer(TOKEN_OPERATOR)).json(body_empty)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // message > 8192 bytes → 400
        const r = try (try (try h.post(steer_idle).bearer(TOKEN_OPERATOR)).json(body_toolong)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── Idle zombie happy path (needs Redis for steer-message queue) ─────────────

test "integration: zombie steer idle — queued, execution_active=false" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/steer", .{ TEST_WORKSPACE_ID, ZOMBIE_IDLE });
    defer ALLOC.free(url);

    const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"proceed to phase 2\"}")).send();
    defer r.deinit();

    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"message_queued\":true"));
    try std.testing.expect(r.bodyContains("\"execution_active\":false"));

    // Drop the Redis steer key written by the handler
    _ = h.queue.getDel("zombie:" ++ ZOMBIE_IDLE ++ ":steer") catch {};

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── Active zombie happy path (needs Redis) ──────────────────────────────────

test "integration: zombie steer active — queued, execution_active=true, execution_id surfaced" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/steer", .{ TEST_WORKSPACE_ID, ZOMBIE_ACTIVE });
    defer ALLOC.free(url);

    const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"new objective\"}")).send();
    defer r.deinit();

    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"message_queued\":true"));
    try std.testing.expect(r.bodyContains("\"execution_active\":true"));
    try std.testing.expect(r.bodyContains(ACTIVE_EXEC_ID));

    _ = h.queue.getDel("zombie:" ++ ZOMBIE_ACTIVE ++ ":steer") catch {};

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
