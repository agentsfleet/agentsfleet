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

const log = std.log.scoped(.zombie_api);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const MAX_NAME_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64 * 1024; // 64KB

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
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
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

    const rows = fetchZombieListOnConn(conn, hx.alloc, workspace_id) catch |err| {
        log.err("zombie.list_failed err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows, .total = rows.len });
}

const ZombieListRow = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
};

fn fetchZombieListOnConn(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8) ![]ZombieListRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, status, created_at, updated_at
        \\FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC
    , .{workspace_id}));
    defer q.deinit();
    return collectZombieRows(alloc, &q);
}

fn collectZombieRows(alloc: std.mem.Allocator, q: *PgQuery) ![]ZombieListRow {
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
    return rows.toOwnedSlice(alloc);
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
