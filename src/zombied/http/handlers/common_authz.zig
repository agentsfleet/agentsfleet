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
    var q = PgQuery.from(blk: {
        if (principal.tenant_id) |tenant_id| {
            break :blk conn.query(
                "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 AND tenant_id = $2",
                .{ workspace_id, tenant_id },
            ) catch return false;
        }
        break :blk conn.query(
            "SELECT 1 FROM core.workspaces WHERE workspace_id = $1",
            .{workspace_id},
        ) catch return false;
    });
    defer q.deinit();
    const row = (q.next() catch return false) orelse return false;
    _ = row;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return false;
    }
    return true;
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    _ = conn.exec("SELECT set_config('app.current_tenant_id', $1, false)", .{tenant_id}) catch return false;
    return true;
}

const UUID_TEXT_LEN: usize = 36;

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    var tenant_buf: [UUID_TEXT_LEN]u8 = undefined;
    const tenant_id = principal.tenant_id orelse blk: {
        var lookup = PgQuery.from(conn.query("SELECT tenant_id::text FROM core.workspaces WHERE workspace_id = $1", .{workspace_id}) catch return false);
        defer lookup.deinit();
        const row = (lookup.next() catch return false) orelse return false;
        const looked_up = row.get([]u8, 0) catch return false;
        if (looked_up.len != UUID_TEXT_LEN) return false;
        @memcpy(tenant_buf[0..], looked_up);
        break :blk tenant_buf[0..];
    };
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
