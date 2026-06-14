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
    // No tenant context at all → forbidden without touching the DB.
    if (hx.principal.user_id == null and hx.principal.tenant_id == null) {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = fetchWorkspacesForPrincipal(conn, hx.alloc, hx.principal) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows, .total = rows.len });
}

const WorkspaceRow = struct {
    id: []const u8,
    name: ?[]const u8,
    created_at: i64,
};

// Hard cap on the tenant workspace result set. Every tenant today sits
// well below this; the cap exists so a misconfigured import or runaway
// test fixture can't spill an unbounded result into the ArrayList.
const MAX_TENANT_WORKSPACES: u32 = 200;

// One round trip: resolve the caller's tenant and list its workspaces in a
// single query. The subject→tenant lookup is authoritative — it overrides a
// stale JWT tenant claim (see the override regression test); the JWT claim is
// only a COALESCE fallback for a caller with no users row yet (fresh signup).
const WS_HEAD = "SELECT workspace_id::text, name, created_at FROM core.workspaces WHERE tenant_id = ";
const DB_TENANT_OF_SUBJECT = "(SELECT tenant_id FROM core.users WHERE oidc_subject = $1)";
const WS_TAIL = std.fmt.comptimePrint(
    " ORDER BY created_at ASC, workspace_id ASC LIMIT {d}",
    .{MAX_TENANT_WORKSPACES},
);

// $1 = oidc_subject, $2 = JWT tenant claim. DB mapping wins; claim is fallback.
const SQL_BY_SUBJECT_OR_CLAIM = WS_HEAD ++ "COALESCE(" ++ DB_TENANT_OF_SUBJECT ++ ", $2::uuid)" ++ WS_TAIL;
// $1 = oidc_subject, no claim available.
const SQL_BY_SUBJECT = WS_HEAD ++ DB_TENANT_OF_SUBJECT ++ WS_TAIL;
// $1 = tenant_id claim, no user subject (e.g. agent/API-key principal).
const SQL_BY_TENANT = WS_HEAD ++ "$1::uuid" ++ WS_TAIL;

fn fetchWorkspacesForPrincipal(conn: *pg.Conn, alloc: std.mem.Allocator, principal: common.AuthPrincipal) ![]WorkspaceRow {
    if (principal.user_id) |subject| {
        if (principal.tenant_id) |claim| {
            return fetchWorkspaces(conn, alloc, SQL_BY_SUBJECT_OR_CLAIM, .{ subject, claim });
        }
        return fetchWorkspaces(conn, alloc, SQL_BY_SUBJECT, .{subject});
    }
    return fetchWorkspaces(conn, alloc, SQL_BY_TENANT, .{principal.tenant_id.?});
}

fn fetchWorkspaces(conn: *pg.Conn, alloc: std.mem.Allocator, comptime sql: []const u8, args: anytype) ![]WorkspaceRow {
    var q = PgQuery.from(try conn.query(sql, args));
    defer q.deinit();

    var rows: std.ArrayList(WorkspaceRow) = .empty;
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
