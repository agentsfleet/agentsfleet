// M2_001 / M24_001: Zombie CRUD API — create, list, delete, status.
//
// POST   /v1/workspaces/{ws}/zombies       → innerCreateZombie
// GET    /v1/workspaces/{ws}/zombies       → innerListZombies
// DELETE /v1/workspaces/{ws}/zombies/{id}  → innerDeleteZombie

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");
const keyset_cursor = @import("../../../zombie/activity_cursor.zig");

const log = std.log.scoped(.zombie_api);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const MAX_NAME_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64 * 1024; // 64KB

const DEFAULT_LIST_PAGE_LIMIT: u32 = 20;
const MAX_LIST_PAGE_LIMIT: u32 = 100;

// ── Create Zombie ─────────────────────────────────────────────────────

// M24_001: workspace_id comes from URL path, not body (RULE RAD §4).
const CreateBody = struct {
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
};

fn parseCreateBody(hx: Hx, req: *httpz.Request) ?CreateBody {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(CreateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn validateCreateFields(hx: Hx, b: CreateBody) bool {
    if (b.name.len == 0 or b.name.len > MAX_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_NAME_REQUIRED);
        return false;
    }
    if (b.source_markdown.len == 0 or b.source_markdown.len > MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_SOURCE_REQUIRED);
        return false;
    }
    if (b.config_json.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED);
        return false;
    }
    return true;
}

pub fn innerCreateZombie(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = parseCreateBody(hx, req) orelse return;
    if (!validateCreateFields(hx, body)) return;

    _ = zombie_config.parseZombieConfig(hx.alloc, body.config_json) catch {
        hx.fail(ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const zombie_id = id_format.generateZombieId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    insertZombieOnConn(conn, workspace_id, body, zombie_id, now_ms) catch |err| {
        if (isUniqueViolation(err)) {
            hx.fail(ec.ERR_ZOMBIE_NAME_EXISTS, ec.MSG_ZOMBIE_NAME_EXISTS);
            return;
        }
        log.err("zombie.create_failed err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("zombie.created id={s} name={s} workspace={s}", .{ zombie_id, body.name, workspace_id });
    hx.ok(.created, .{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.active.toSlice() });
}

fn insertZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, body: CreateBody, zombie_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5::jsonb, $6, $7, $8)
    , .{ zombie_id, workspace_id, body.name, body.source_markdown, body.config_json, zombie_config.ZombieStatus.active.toSlice(), now_ms, now_ms });
}

fn isUniqueViolation(_: anyerror) bool {
    // pg.Pool returns error.PGError for all Postgres errors (connection, constraint, cast).
    // We cannot distinguish unique_violation (SQLSTATE 23505) from other PGErrors
    // because pg.Pool does not expose structured SQLSTATE codes.
    // Return false to let the caller surface a 500 instead of a misleading 409.
    return false;
}

// ── List Zombies ──────────────────────────────────────────────────────

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
    // Composite keyset cursor prevents silent skips when multiple zombies
    // land on the same millisecond timestamp. See activity_cursor.zig.
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

// ── Delete Zombie ─────────────────────────────────────────────────────

pub fn innerDeleteZombie(hx: Hx, _: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const updated = killZombieOnConn(conn, workspace_id, zombie_id) catch |err| {
        log.err("zombie.delete_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (!updated) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    log.info("zombie.killed id={s} workspace={s}", .{ zombie_id, workspace_id });
    hx.ok(.ok, .{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.killed.toSlice() });
}

fn killZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8) !bool {
    const now_ms = std.time.milliTimestamp();
    // Scope to workspace_id to prevent cross-tenant deletes (RULE WAUTH defence in depth).
    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET status = $1, updated_at = $2
        \\WHERE id = $3::uuid AND workspace_id = $4::uuid AND status != $1
        \\RETURNING id
    , .{ zombie_config.ZombieStatus.killed.toSlice(), now_ms, zombie_id, workspace_id }));
    defer q.deinit();
    return try q.next() != null;
}

// validateCreateFields is tested via integration tests (DB-backed).
// It requires a fully initialized httpz.Response which cannot be mocked in unit tests.

test "isUniqueViolation always returns false (no SQLSTATE introspection)" {
    try std.testing.expect(!isUniqueViolation(error.PGError));
    try std.testing.expect(!isUniqueViolation(error.OutOfMemory));
}
