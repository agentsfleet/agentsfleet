// Regression test for the workspace->tenant RLS-context resolution in
// `common.authorizeWorkspaceAndSetTenantContext`. A principal whose tenant_id is
// null (e.g. api_key auth) takes the lookup branch that reads the tenant out of
// a pg.Result and feeds it to `set_config('app.current_tenant_id', ...)`. That
// branch used to serialize a slice into a freed result buffer (use-after-free)
// as the RLS tenant context — a cross-tenant security hazard. This proves the
// context ends up as the workspace's real tenant, not freed garbage.
//
// Requires LIVE_DB=1; skipped (error.SkipZigTest) when TEST_DATABASE_URL is unset.

const std = @import("std");
const common = @import("common.zig");
const base = @import("../../db/test_fixtures.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-c0ffee000001";

test "authorizeWorkspaceAndSetTenantContext sets the workspace's real tenant for a null-tenant principal" {
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

    // tenant_id == null forces the workspace->tenant lookup branch (the UAF site).
    const principal = common.AuthPrincipal{ .mode = .api_key, .role = .admin, .tenant_id = null };
    _ = common.authorizeWorkspaceAndSetTenantContext(conn, principal, WORKSPACE_ID);

    // The RLS session context must be the workspace's real tenant — proving the
    // looked-up id was copied out of the result buffer before it was freed.
    var q = PgQuery.from(try conn.query("SELECT current_setting('app.current_tenant_id', true)", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TenantContextRowMissing;
    const ctx_tenant = try row.get([]const u8, 0);
    try std.testing.expectEqualStrings(base.TEST_TENANT_ID, ctx_tenant);
}
