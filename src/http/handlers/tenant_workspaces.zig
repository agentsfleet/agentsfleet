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
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const resolved_tenant_id = resolveTenantId(conn, hx.alloc, hx.principal) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer if (resolved_tenant_id.owned) |owned| hx.alloc.free(owned);
    const tenant_id = resolved_tenant_id.value orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const rows = fetchWorkspacesOnConn(conn, hx.alloc, tenant_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows, .total = rows.len });
}

const TenantIdResolution = struct {
    value: ?[]const u8,
    owned: ?[]u8 = null,
};

const WorkspaceRow = struct {
    id: []const u8,
    name: ?[]const u8,
    created_at: i64,
};

// Hard cap on the tenant workspace result set. Every tenant today sits
// well below this; the cap exists so a misconfigured import or runaway
// test fixture can't spill an unbounded result into the ArrayList.
const MAX_TENANT_WORKSPACES: u32 = 200;

const LIST_WORKSPACES_SQL = std.fmt.comptimePrint(
    "SELECT workspace_id::text, name, created_at " ++
        "FROM core.workspaces WHERE tenant_id = $1::uuid " ++
        "ORDER BY created_at ASC, workspace_id ASC LIMIT {d}",
    .{MAX_TENANT_WORKSPACES},
);

fn resolveTenantId(conn: *pg.Conn, alloc: std.mem.Allocator, principal: common.AuthPrincipal) !TenantIdResolution {
    if (principal.user_id) |subject| {
        var q = PgQuery.from(try conn.query(
            "SELECT tenant_id::text FROM core.users WHERE oidc_subject = $1 LIMIT 1",
            .{subject},
        ));
        defer q.deinit();
        defer q.drain();

        if (try q.next()) |row| {
            const tenant_id = try alloc.dupe(u8, try row.get([]const u8, 0));
            return .{ .value = tenant_id, .owned = tenant_id };
        }
    }

    if (principal.tenant_id) |tenant_id| return .{ .value = tenant_id };
    return .{ .value = null };
}

fn fetchWorkspacesOnConn(conn: *pg.Conn, alloc: std.mem.Allocator, tenant_id: []const u8) ![]WorkspaceRow {
    var q = PgQuery.from(try conn.query(LIST_WORKSPACES_SQL, .{tenant_id}));
    defer q.deinit();

    var rows: std.ArrayList(WorkspaceRow) = .{};
    errdefer {
        for (rows.items) |r| {
            alloc.free(r.id);
            if (r.name) |n| alloc.free(n);
        }
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const name_raw = row.get(?[]const u8, 1) catch null;
        const name: ?[]const u8 = if (name_raw) |n| try alloc.dupe(u8, n) else null;
        errdefer if (name) |n| alloc.free(n);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .created_at = try row.get(i64, 2),
        });
    }
    return rows.toOwnedSlice(alloc);
}
