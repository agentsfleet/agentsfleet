//! GET /v1/tenants/me/workspaces — tenant-scoped workspace list.
//!
//! Returns every workspace in the caller's tenant. Powers the dashboard
//! workspace switcher — users with memberships in a multi-workspace
//! tenant get the full list; users in a single-workspace tenant get a
//! one-item response.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

pub fn innerListTenantWorkspaces(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = fetchWorkspacesOnConn(conn, hx.alloc, tenant_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows, .total = rows.len });
}

const WorkspaceRow = struct {
    id: []const u8,
    name: ?[]const u8,
    repo_url: ?[]const u8,
    created_at: i64,
};

fn fetchWorkspacesOnConn(conn: *pg.Conn, alloc: std.mem.Allocator, tenant_id: []const u8) ![]WorkspaceRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, name, repo_url, created_at
        \\FROM core.workspaces
        \\WHERE tenant_id = $1::uuid
        \\ORDER BY created_at ASC, workspace_id ASC
    , .{tenant_id}));
    defer q.deinit();

    var rows: std.ArrayList(WorkspaceRow) = .{};
    errdefer {
        for (rows.items) |r| {
            alloc.free(r.id);
            if (r.name) |n| alloc.free(n);
            if (r.repo_url) |u| alloc.free(u);
        }
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const name_raw = row.get(?[]const u8, 1) catch null;
        const name: ?[]const u8 = if (name_raw) |n| try alloc.dupe(u8, n) else null;
        errdefer if (name) |n| alloc.free(n);
        const url_raw = row.get(?[]const u8, 2) catch null;
        const repo_url: ?[]const u8 = if (url_raw) |u| try alloc.dupe(u8, u) else null;
        errdefer if (repo_url) |u| alloc.free(u);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .repo_url = repo_url,
            .created_at = try row.get(i64, 3),
        });
    }
    return rows.toOwnedSlice(alloc);
}
