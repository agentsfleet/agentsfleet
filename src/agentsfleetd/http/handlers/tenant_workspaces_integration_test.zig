//! HTTP integration tests for GET /v1/tenants/me/workspaces.
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../auth/middleware/mod.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OVERRIDE_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21";
const OVERRIDE_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31";
const OVERRIDE_USER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41";
const TOKEN_SUBJECT = "user_test";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLnRlc3QuYWdlbnRzZmxlZXQubmV0IiwiYXVkIjoiaHR0cHM6Ly9hcGkuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwLCJzY29wZXMiOiJmbGVldDphZG1pbiBjcmVkZW50aWFsOndyaXRlIGFwaWtleTphZG1pbiBmbGVldGtleTp3cml0ZSBncmFudDp3cml0ZSBjb25uZWN0b3I6d3JpdGUgYmlsbGluZzpyZWFkIGFwcHJvdmFsOnJlc29sdmUgd29ya3NwYWNlOmFkbWluIHRlbXBsYXRlOndyaXRlIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIn19.clzrJQSbL5tON0PQQwuJYCRDJVDHiebt40X0wYNsN93A6KlNcLO2I_zREIXn2aUI8HAN0WaVJKGHuh1RXuQ-4Fw4wUS7UFIlrY_4DWKkTg6WCbAXxhwe90ScOn9Q5oXUfDLTbpMGw1sFgLe67qy2QPdyH_yephKyjArBnwJQqMbXtb-uKXN66lcrgHlR-KoBGzqkDHyc5bVy9CPKiLgbzZQac1mug53gc8zOZeAFlfgTXTWdSn65f37Cd-vmbGngrhY6sH2oZcUGOlXPiZtyw7jgWyp6tL9gLiDEwwLbQFkUqVvUjjhmkY8-LG7nna-ratPpt5UK3r7WB4bjREbsyQ";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenant(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'TenantWsTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
}

fn seedWorkspace(conn: *pg.Conn, ws_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ ws_id, TEST_TENANT_ID, now_ms });
}

fn seedOverrideTenant(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'TenantWsOverride', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ OVERRIDE_TENANT_ID, now_ms });
}

fn seedOverrideWorkspace(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ OVERRIDE_WORKSPACE_ID, OVERRIDE_TENANT_ID, now_ms });
}

fn seedOverrideUser(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'tenant-workspaces-override@test.agentsfleet', $4, $4)
        \\ON CONFLICT (oidc_subject) DO UPDATE SET tenant_id = EXCLUDED.tenant_id, updated_at = EXCLUDED.updated_at
    , .{ OVERRIDE_USER_ID, OVERRIDE_TENANT_ID, TOKEN_SUBJECT, now_ms });
}

fn cleanupOverride(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM workspaces WHERE tenant_id = $1::uuid", .{OVERRIDE_TENANT_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM memberships WHERE user_id = $1::uuid", .{OVERRIDE_USER_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM users WHERE user_id = $1::uuid", .{OVERRIDE_USER_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1::uuid", .{OVERRIDE_TENANT_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "integration: tenant workspaces — auth, list, scoping" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    cleanupOverride(conn);
    defer cleanupOverride(conn);
    try seedTenant(conn, now_ms);
    try seedWorkspace(conn, TEST_WORKSPACE_ID, now_ms);

    const url = "/v1/tenants/me/workspaces";

    { // Happy path — bearer principal sees their tenant's workspace.
        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"items\""));
        try std.testing.expect(r.bodyContains(TEST_WORKSPACE_ID));
    }

    { // DB subject mapping overrides a stale token tenant claim.
        try seedOverrideTenant(conn, now_ms);
        try seedOverrideWorkspace(conn, now_ms);
        try seedOverrideUser(conn, now_ms);

        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(OVERRIDE_WORKSPACE_ID));
        try std.testing.expect(!r.bodyContains(TEST_WORKSPACE_ID));
    }

    { // No token → 401.
        const r = try h.get(url).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
}
