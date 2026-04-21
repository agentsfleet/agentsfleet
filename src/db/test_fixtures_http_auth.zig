/// test_fixtures_http_auth.zig — HTTP-layer workspace auth integration fixtures.
///
/// Shared tenant + two workspace UUIDs used by:
///   - src/http/workspace_guards.zig  (isWorkspaceCreator integration tests)
///   - src/http/handlers/common.zig   (authorizeWorkspace scoping tests)
///
/// Both suites exercise the real `core.workspaces` / `core.tenants` tables
/// against `authorizeWorkspace` + `isWorkspaceCreator` SQL queries, so
/// centralising the IDs and teardown here keeps the production source files
/// free of fixture scaffolding.
///
/// Usage per test:
///
///   try http_auth.seedTenant(conn);
///   try http_auth.seedGuardWorkspace(conn, http_auth.WS_PRIMARY, "user_alice");
///   defer http_auth.cleanup(conn);
///
/// `cleanup` removes both workspaces and the tenant in FK-safe order and is
/// idempotent — safe to call before seeding as a "reset previous run" bookend.

const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
pub const WS_PRIMARY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
pub const WS_SECONDARY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12";

/// UUID reserved as "definitely absent" for the `returns false for
/// non-existent workspace` assertion. Never inserted anywhere.
pub const WS_ABSENT = "0195b4ba-8d3a-7f13-8abc-999900000000";

/// A tenant UUID that exists nowhere — used by tests that assert "wrong
/// tenant_id blocks even correct creator."
pub const TENANT_UNRELATED = "0195b4ba-8d3a-7f13-8abc-000000000099";

/// Delete both workspaces, then the tenant. FK order: workspaces → tenants
/// (NO ACTION). Idempotent; safe as both the pre-seed reset and the
/// post-test teardown.
pub fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM core.workspaces WHERE workspace_id IN ($1::uuid, $2::uuid)",
        .{ WS_PRIMARY, WS_SECONDARY },
    ) catch {};
    _ = conn.exec(
        "DELETE FROM core.tenants WHERE tenant_id = $1::uuid",
        .{TENANT_ID},
    ) catch {};
}

/// Insert the shared auth-test tenant. Idempotent via ON CONFLICT DO NOTHING.
pub fn seedTenant(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "http-auth-tests");
}

/// Seed a workspace under the shared tenant with no explicit creator. Used
/// by the `authorizeWorkspace` scoping tests that don't care about ownership.
pub fn seedScopeWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedWorkspaceWithTenant(conn, workspace_id, TENANT_ID);
}

/// Seed a workspace under the shared tenant with explicit `created_by`.
/// Used by the `isWorkspaceCreator` tests to assert owner-override behavior.
pub fn seedGuardWorkspace(
    conn: *pg.Conn,
    workspace_id: []const u8,
    created_by: ?[]const u8,
) !void {
    try base.seedWorkspaceWithCreator(conn, workspace_id, TENANT_ID, created_by);
}
