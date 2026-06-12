const std = @import("std");
const constants = @import("common");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const db = @import("../../db/pool.zig");
const AuthPrincipal = @import("../../auth/principal.zig").AuthPrincipal;

pub fn getZombieWorkspaceId(conn: *pg.Conn, alloc: std.mem.Allocator, zombie_id: []const u8) ?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid LIMIT 1
    , .{zombie_id}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const ws = row.get([]u8, 0) catch return null;
    return alloc.dupe(u8, ws) catch null;
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    // Fail closed without a tenant. A null tenant_id must NEVER degrade to an
    // unscoped existence check — that authorizes the caller against any tenant's
    // workspace (cross-tenant IDOR). The only null-tenant principals are an
    // unprovisioned Clerk session (before the user.created metadata writeback
    // lands) and runner tokens; the former is exactly the attacker, and runners
    // authorize via runnerBearer against fleet.runners, never through here.
    const tenant_id = principal.tenant_id orelse return false;

    var q = PgQuery.from(conn.query(
        "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 AND tenant_id = $2",
        .{ workspace_id, tenant_id },
    ) catch return false);
    defer q.deinit();
    _ = (q.next() catch return false) orelse return false;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return false;
    }
    return true;
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    _ = conn.exec("SELECT set_config('app.current_tenant_id', $1, false)", .{tenant_id}) catch return false;
    return true;
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    // Fail closed without a tenant — never resolve the tenant from the *target*
    // workspace for a null-tenant principal. That fallback set the RLS session
    // context to the workspace owner's (victim's) tenant and then authorized,
    // which was the cross-tenant IDOR. The inner authorizeWorkspace enforces the
    // same invariant; guarding here also skips the pointless lookup + context write.
    const tenant_id = principal.tenant_id orelse return false;
    if (!setTenantSessionContext(conn, tenant_id)) return false;
    return authorizeWorkspace(conn, principal, workspace_id);
}

pub fn openHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = constants.env.testLiveValue("TEST_DATABASE_URL") orelse
        constants.env.testLiveValue("DATABASE_URL") orelse return null;
    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(constants.globalIo(), alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}
