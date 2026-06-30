// Tenant-isolation regression test for `common.authorizeWorkspaceAndSetTenantContext`.
//
// A null-tenant principal (an unprovisioned Clerk session before the user.created
// metadata writeback lands) used to take a fallback branch that resolved the
// tenant FROM THE TARGET WORKSPACE, set `set_config('app.current_tenant_id', ...)`
// to that workspace owner's (victim's) tenant, and then authorized — a cross-tenant
// IDOR (a freshly-signed-up user could read/write any tenant's workspace by ID and
// mint a durable fleet key against it). The fix fails closed: a null tenant_id is
// never resolved from the workspace; the call returns false and leaves the RLS
// context untouched. (The earlier use-after-free this branch was hardened against
// is dissolved — the unsafe lookup no longer exists.)
//
// Requires LIVE_DB=1; skipped (error.SkipZigTest) when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const common = @import("common.zig");
const base = @import("../../db/test_fixtures.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-c0ffee000001";
// A different, valid-shaped workspace id used only as a scope mismatch.
const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-c0ffee0000ff";
// Distinct, valid-shaped UUID used as a pre-set RLS context the call must NOT touch.
const SENTINEL_TENANT = "00000000-0000-7000-8000-000000000000";

// Reads app.current_tenant_id and compares it in-place (before q.deinit frees the
// result buffer) so no borrowed slice escapes — returning the slice would be a UAF.
fn tenantCtxEquals(conn: *pg.Conn, expected: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT current_setting('app.current_tenant_id', true)", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TenantContextRowMissing;
    return std.mem.eql(u8, try row.get([]const u8, 0), expected);
}

test "authorizeWorkspaceAndSetTenantContext fails closed for a null-tenant principal (no cross-tenant context)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    defer {
        base.teardownWorkspace(conn, WORKSPACE_ID);
        base.teardownTenant(conn);
    }

    // Pre-set a sentinel RLS context so we can prove the call leaves it untouched
    // (i.e. it never resolved + set the workspace owner's tenant).
    try std.testing.expect(common.setTenantSessionContext(conn, SENTINEL_TENANT));

    // A null-tenant principal — even against a workspace that really exists —
    // must be denied and must not have its tenant inferred from that workspace.
    const principal = common.AuthPrincipal{ .mode = .jwt_oidc, .tenant_id = null };
    try std.testing.expect(!common.authorizeWorkspaceAndSetTenantContext(conn, principal, WORKSPACE_ID));

    // RLS context is untouched: still the sentinel, never the workspace's real tenant.
    var q = PgQuery.from(try conn.query("SELECT current_setting('app.current_tenant_id', true)", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TenantContextRowMissing;
    const ctx_tenant = try row.get([]const u8, 0);
    try std.testing.expectEqualStrings(SENTINEL_TENANT, ctx_tenant);
    try std.testing.expect(!std.mem.eql(u8, ctx_tenant, base.TEST_TENANT_ID));
}

test "authorizeWorkspaceAndSetTenantContext writes the RLS context only on a successful authorization" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    defer {
        base.teardownWorkspace(conn, WORKSPACE_ID);
        base.teardownTenant(conn);
    }

    // Denied: a valid-tenant principal scoped to a DIFFERENT workspace fails the
    // scope check. The RLS context must stay the pre-set sentinel — writing it on
    // the failure path would leak the caller's tenant onto the pooled connection.
    try std.testing.expect(common.setTenantSessionContext(conn, SENTINEL_TENANT));
    const denied = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_scope_id = OTHER_WORKSPACE_ID,
    };
    try std.testing.expect(!common.authorizeWorkspaceAndSetTenantContext(conn, denied, WORKSPACE_ID));
    try std.testing.expect(try tenantCtxEquals(conn, SENTINEL_TENANT));

    // Authorized: the context IS written, to the caller's own tenant.
    const authorized = common.AuthPrincipal{ .mode = .jwt_oidc, .tenant_id = base.TEST_TENANT_ID };
    try std.testing.expect(common.authorizeWorkspaceAndSetTenantContext(conn, authorized, WORKSPACE_ID));
    try std.testing.expect(try tenantCtxEquals(conn, base.TEST_TENANT_ID));
}
