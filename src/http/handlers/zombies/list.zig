//! GET /v1/workspaces/{ws}/zombies — paginated list of zombies in a workspace.
//!
//! Composite keyset pagination: cursor encodes (created_at_ms, id) so multiple
//! zombies installed on the same millisecond are not silently skipped. See
//! `src/zombie/activity_cursor.zig` for the cursor format.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const keyset_cursor = @import("../../../zombie/activity_cursor.zig");

const log = std.log.scoped(.zombie_api);

const Hx = hx_mod.Hx;

const DEFAULT_LIST_PAGE_LIMIT: u32 = 20;
const MAX_LIST_PAGE_LIMIT: u32 = 100;

pub fn innerListZombies(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const qs = req.query() catch null;
    const limit = if (qs) |q| parseLimitFromQs(q) else DEFAULT_LIST_PAGE_LIMIT;
    const cursor = if (qs) |q| q.get("cursor") else null;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const page = fetchZombiePageOnConn(conn, hx.alloc, workspace_id, cursor, limit) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "Invalid cursor format");
            return;
        }
        log.err("zombie.list_failed err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = page.rows, .total = page.rows.len, .cursor = page.next_cursor });
}

fn parseLimitFromQs(qs: anytype) u32 {
    const limit_str = qs.get("limit") orelse return DEFAULT_LIST_PAGE_LIMIT;
    const parsed = std.fmt.parseInt(u32, limit_str, 10) catch return DEFAULT_LIST_PAGE_LIMIT;
    // Treat limit=0 as the default. With LIMIT 0 the cursor guard reports
    // no more pages, so callers would stop paginating even when rows exist.
    return if (parsed == 0) DEFAULT_LIST_PAGE_LIMIT else parsed;
}

const ZombieListRow = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
};

const ZombiePage = struct {
    rows: []ZombieListRow,
    next_cursor: ?[]const u8,
};

fn fetchZombiePageOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !ZombiePage {
    const page_limit = @min(limit, MAX_LIST_PAGE_LIMIT);
    return if (cursor) |c|
        fetchZombiePageAfter(conn, alloc, workspace_id, c, page_limit)
    else
        fetchZombiePageFirst(conn, alloc, workspace_id, page_limit);
}

fn fetchZombiePageFirst(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    limit: u32,
) !ZombiePage {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, status, created_at, updated_at
        \\FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $2
    , .{ workspace_id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectZombiePage(alloc, &q, limit);
}

fn fetchZombiePageAfter(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cursor: []const u8,
    limit: u32,
) !ZombiePage {
    const parsed = keyset_cursor.parse(cursor) catch return error.InvalidCursor;

    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, status, created_at, updated_at
        \\FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\  AND (created_at < $2 OR (created_at = $2 AND id::text < $3))
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $4
    , .{ workspace_id, parsed.created_at_ms, parsed.id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectZombiePage(alloc, &q, limit);
}

fn collectZombiePage(alloc: std.mem.Allocator, q: *PgQuery, limit: u32) !ZombiePage {
    var rows: std.ArrayList(ZombieListRow) = .{};
    errdefer {
        for (rows.items) |r| {
            alloc.free(r.id);
            alloc.free(r.name);
            alloc.free(r.status);
        }
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const name = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(name);
        const status = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(status);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .status = status,
            .created_at = try row.get(i64, 3),
            .updated_at = try row.get(i64, 4),
        });
    }
    const owned = try rows.toOwnedSlice(alloc);
    errdefer {
        for (owned) |r| {
            alloc.free(r.id);
            alloc.free(r.name);
            alloc.free(r.status);
        }
        alloc.free(owned);
    }

    const next_cursor: ?[]const u8 = if (owned.len == limit and owned.len > 0) blk: {
        const last = owned[owned.len - 1];
        break :blk try keyset_cursor.format(alloc, .{ .created_at_ms = last.created_at, .id = last.id });
    } else null;

    return ZombiePage{ .rows = owned, .next_cursor = next_cursor };
}
