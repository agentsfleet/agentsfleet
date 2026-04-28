// HTTP integration tests for the M42 events read endpoints:
//   GET /v1/workspaces/{ws}/zombies/{id}/events
//   GET /v1/workspaces/{ws}/events
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. SSE happy-path
// is exercised by tests that need Redis and is left to slice 11; the auth
// surface and the "no-bearer 401" check still run here without Redis.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

// Reuse the JWT signing fixture (kid + JWKS + tenant/workspace claims) from
// steer_integration_test.zig so we don't have to mint a fresh signature.
// Zombies live in this same workspace; the test isolates state by zombie_id.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const ZOMBIE_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb01";
const ZOMBIE_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb02";
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
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/events', 'main', false, 1, $3, $3)
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

    const r = try (try h.get(url)).send();
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

    const r = try (try (try h.get(url)).bearer(TOKEN_OPERATOR)).send();
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

    const r = try (try (try h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try std.testing.expect(r.bodyContains("invalid_since_format"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── SSE auth (no Redis) ─────────────────────────────────────────────────────

test "integration: events stream — no bearer → 401 (cookie path lands with dashboard slice)" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/events/stream", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);

    const r = try (try h.get(url)).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
