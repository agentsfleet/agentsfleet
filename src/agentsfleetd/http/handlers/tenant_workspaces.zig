//! GET /v1/tenants/me/workspaces — tenant-scoped workspace list.
//!
//! Returns stable oldest-first cursor pages. An exact `name` filter supports
//! create reconciliation without downloading or scanning the tenant's list.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const keyset_cursor = @import("../../fleet_runtime/keyset_cursor.zig");

const Hx = hx_mod.Hx;

const DEFAULT_PAGE_LIMIT: u32 = 50;
const MAX_PAGE_LIMIT: u32 = 100;
const MAX_WORKSPACE_NAME_CODEPOINTS: usize = 128;
const TENANT_BUFFER_BYTES: usize = 64;
const DETAIL_TENANT_REQUIRED = "Tenant context required";
const DETAIL_MALFORMED_QUERY = "Malformed query string";
const DETAIL_INVALID_LIMIT = "Limit must be between 1 and 100";
const DETAIL_INVALID_CURSOR = "Invalid starting_after cursor";
const DETAIL_INVALID_NAME = "Name must be between 1 and 128 Unicode code points";

const SQL_FIRST =
    \\SELECT w.id::text AS id, w.name, w.created_at
    \\FROM core.workspaces AS w
    \\WHERE w.tenant_id = $1::uuid
    \\ORDER BY w.created_at ASC, w.id ASC
    \\LIMIT $2
;
const SQL_AFTER =
    \\SELECT w.id::text AS id, w.name, w.created_at
    \\FROM core.workspaces AS w
    \\WHERE w.tenant_id = $1::uuid
    \\  AND (w.created_at, w.id) > ($2, $3::uuid)
    \\ORDER BY w.created_at ASC, w.id ASC
    \\LIMIT $4
;
const SQL_FIRST_BY_NAME =
    \\SELECT w.id::text AS id, w.name, w.created_at
    \\FROM core.workspaces AS w
    \\WHERE w.tenant_id = $1::uuid AND w.name = $2
    \\ORDER BY w.created_at ASC, w.id ASC
    \\LIMIT $3
;
const SQL_AFTER_BY_NAME =
    \\SELECT w.id::text AS id, w.name, w.created_at
    \\FROM core.workspaces AS w
    \\WHERE w.tenant_id = $1::uuid AND w.name = $2
    \\  AND (w.created_at, w.id) > ($3, $4::uuid)
    \\ORDER BY w.created_at ASC, w.id ASC
    \\LIMIT $5
;

pub fn innerListTenantWorkspaces(hx: Hx, req: *httpz.Request) void {
    if (hx.principal.user_id == null and hx.principal.tenant_id == null) {
        hx.fail(ec.ERR_FORBIDDEN, DETAIL_TENANT_REQUIRED);
        return;
    }

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_MALFORMED_QUERY);
        return;
    };
    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_LIMIT);
        return;
    };
    const cursor = parseCursor(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_CURSOR);
        return;
    };
    const name = parseName(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_NAME);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var tenant_buf: [TENANT_BUFFER_BYTES]u8 = undefined;
    const tenant_id = common.resolvePrincipalTenant(
        conn,
        hx.principal,
        &tenant_buf,
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_FORBIDDEN, DETAIL_TENANT_REQUIRED);
        return;
    };

    var page = fetchWorkspacePage(
        conn,
        hx.alloc,
        tenant_id,
        name,
        cursor,
        limit,
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer page.deinit(hx.alloc);

    hx.ok(.ok, .{
        .items = page.items,
        .tenant_id = tenant_id,
        .total = null,
        .next_cursor = page.next_cursor,
    });
}

const WorkspaceRow = struct {
    id: []const u8,
    name: ?[]const u8,
    created_at: i64,
};

fn freeWorkspaceRows(alloc: std.mem.Allocator, rows: []const WorkspaceRow) void {
    for (rows) |row| {
        alloc.free(row.id);
        if (row.name) |name| alloc.free(name);
    }
}

const WorkspacePage = struct {
    items: []WorkspaceRow,
    next_cursor: ?[]const u8,

    fn deinit(self: *WorkspacePage, alloc: std.mem.Allocator) void {
        freeWorkspaceRows(alloc, self.items);
        alloc.free(self.items);
        if (self.next_cursor) |cursor| alloc.free(cursor);
    }
};

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return DEFAULT_PAGE_LIMIT;
    const parsed = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (parsed == 0 or parsed > MAX_PAGE_LIMIT) return error.InvalidLimit;
    return parsed;
}

fn parseCursor(qs: anytype) !?keyset_cursor.Cursor {
    const raw = qs.get("starting_after") orelse return null;
    const cursor = keyset_cursor.parse(raw) catch return error.InvalidCursor;
    if (!id_format.isSupportedWorkspaceId(cursor.id)) return error.InvalidCursor;
    return cursor;
}

fn parseName(qs: anytype) !?[]const u8 {
    const name = qs.get("name") orelse return null;
    const view = std.unicode.Utf8View.init(name) catch return error.InvalidName;
    var iterator = view.iterator();
    var codepoints: usize = 0;
    while (iterator.nextCodepoint()) |codepoint| {
        if (codepoint == 0) return error.InvalidName;
        codepoints += 1;
    }
    if (codepoints == 0 or codepoints > MAX_WORKSPACE_NAME_CODEPOINTS) {
        return error.InvalidName;
    }
    return name;
}

fn fetchWorkspacePage(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
    name: ?[]const u8,
    cursor: ?keyset_cursor.Cursor,
    limit: u32,
) !WorkspacePage {
    const query_limit: i64 = @intCast(limit + 1);
    if (name) |exact_name| {
        if (cursor) |after| {
            var q = PgQuery.from(try conn.query(
                SQL_AFTER_BY_NAME,
                .{ tenant_id, exact_name, after.created_at_ms, after.id, query_limit },
            ));
            defer q.deinit();
            return collectWorkspacePage(alloc, &q, limit);
        }
        var q = PgQuery.from(try conn.query(
            SQL_FIRST_BY_NAME,
            .{ tenant_id, exact_name, query_limit },
        ));
        defer q.deinit();
        return collectWorkspacePage(alloc, &q, limit);
    }
    if (cursor) |after| {
        var q = PgQuery.from(try conn.query(
            SQL_AFTER,
            .{ tenant_id, after.created_at_ms, after.id, query_limit },
        ));
        defer q.deinit();
        return collectWorkspacePage(alloc, &q, limit);
    }
    var q = PgQuery.from(try conn.query(SQL_FIRST, .{ tenant_id, query_limit }));
    defer q.deinit();
    return collectWorkspacePage(alloc, &q, limit);
}

fn collectWorkspacePage(
    alloc: std.mem.Allocator,
    query: *PgQuery,
    limit: u32,
) !WorkspacePage {
    var rows: std.ArrayList(WorkspaceRow) = .empty;
    const page_limit: usize = @intCast(limit);
    errdefer {
        freeWorkspaceRows(alloc, rows.items);
        rows.deinit(alloc);
    }

    var has_more = false;
    while (try query.next()) |row| {
        if (rows.items.len == page_limit) {
            has_more = true;
            break;
        }
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const raw_name = try row.get(?[]const u8, 1);
        const name: ?[]const u8 = if (raw_name) |value|
            try alloc.dupe(u8, value)
        else
            null;
        errdefer if (name) |value| alloc.free(value);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .created_at = try row.get(i64, 2),
        });
    }

    const items = try rows.toOwnedSlice(alloc);
    errdefer {
        freeWorkspaceRows(alloc, items);
        alloc.free(items);
    }
    const next_cursor: ?[]const u8 = if (has_more and items.len > 0) blk: {
        const last = items[items.len - 1];
        break :blk try keyset_cursor.format(alloc, .{
            .created_at_ms = last.created_at,
            .id = last.id,
        });
    } else null;
    return .{ .items = items, .next_cursor = next_cursor };
}
