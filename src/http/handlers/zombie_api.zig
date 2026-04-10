// M2_001: Zombie CRUD API — create, list, delete, status.
//
// POST   /v1/zombies/           → handleCreateZombie
// GET    /v1/zombies/           → handleListZombies (workspace_id query param)
// DELETE /v1/zombies/{id}       → handleDeleteZombie

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("common.zig");
const ec = @import("../../errors/codes.zig");
const id_format = @import("../../types/id_format.zig");
const zombie_config = @import("../../zombie/config.zig");

const log = std.log.scoped(.zombie_api);

pub const Context = common.Context;

const MAX_NAME_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64 * 1024; // 64KB

// ── Create Zombie ─────────────────────────────────────────────────────

const CreateBody = struct {
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
    workspace_id: []const u8,
};

fn parseCreateBody(alloc: std.mem.Allocator, req: *httpz.Request, res: *httpz.Response, req_id: []const u8) ?CreateBody {
    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED, req_id);
        return null;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return null;
    const parsed = std.json.parseFromSlice(CreateBody, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON, req_id);
        return null;
    };
    return parsed.value;
}

fn validateCreateFields(b: CreateBody, res: *httpz.Response, req_id: []const u8) bool {
    if (b.name.len == 0 or b.name.len > MAX_NAME_LEN) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_NAME_REQUIRED, req_id);
        return false;
    }
    if (b.source_markdown.len == 0 or b.source_markdown.len > MAX_SOURCE_LEN) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_SOURCE_REQUIRED, req_id);
        return false;
    }
    if (b.config_json.len == 0) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED, req_id);
        return false;
    }
    if (b.workspace_id.len == 0 or !id_format.isSupportedWorkspaceId(b.workspace_id)) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED, req_id);
        return false;
    }
    return true;
}

pub fn handleCreateZombie(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch {
        common.errorResponse(res, .unauthorized, ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED, req_id);
        return;
    };
    _ = principal;

    const body = parseCreateBody(alloc, req, res, req_id) orelse return;
    if (!validateCreateFields(body, res, req_id)) return;

    // Validate config JSON parses correctly
    _ = zombie_config.parseZombieConfig(alloc, body.config_json) catch {
        common.errorResponse(res, .bad_request, ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG, req_id);
        return;
    };

    const zombie_id = id_format.generateZombieId(alloc) catch {
        common.internalOperationError(res, "ID generation failed", req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    insertZombie(ctx.pool, body, zombie_id, now_ms) catch |err| {
        if (isUniqueViolation(err)) {
            common.errorResponse(res, .conflict, ec.ERR_ZOMBIE_NAME_EXISTS, ec.MSG_ZOMBIE_NAME_EXISTS, req_id);
            return;
        }
        log.err("zombie.create_failed err={s} req_id={s}", .{ @errorName(err), req_id });
        common.internalDbError(res, req_id);
        return;
    };

    log.info("zombie.created id={s} name={s} workspace={s}", .{ zombie_id, body.name, body.workspace_id });
    res.status = 201;
    res.json(.{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.active.toSlice() }, .{}) catch {};
}

fn insertZombie(pool: *pg.Pool, body: CreateBody, zombie_id: []const u8, now_ms: i64) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5::jsonb, $6, $7, $8)
    , .{ zombie_id, body.workspace_id, body.name, body.source_markdown, body.config_json, zombie_config.ZombieStatus.active.toSlice(), now_ms, now_ms });
}

fn isUniqueViolation(_: anyerror) bool {
    // pg.Pool returns error.PGError for all Postgres errors (connection, constraint, cast).
    // We cannot distinguish unique_violation (SQLSTATE 23505) from other PGErrors
    // because pg.Pool does not expose structured SQLSTATE codes.
    // Return false to let the caller surface a 500 instead of a misleading 409.
    return false;
}

// ── List Zombies ──────────────────────────────────────────────────────

pub fn handleListZombies(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch {
        common.errorResponse(res, .unauthorized, ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED, req_id);
        return;
    };
    _ = principal;

    const qs = req.query() catch {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED, req_id);
        return;
    };
    const workspace_id = qs.get("workspace_id") orelse {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED, req_id);
        return;
    };
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED, req_id);
        return;
    }

    const rows = fetchZombieList(ctx.pool, alloc, workspace_id) catch |err| {
        log.err("zombie.list_failed err={s} req_id={s}", .{ @errorName(err), req_id });
        common.internalDbError(res, req_id);
        return;
    };

    res.status = 200;
    res.json(.{ .zombies = rows }, .{}) catch {};
}

const ZombieListRow = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
};

fn fetchZombieList(pool: *pg.Pool, alloc: std.mem.Allocator, workspace_id: []const u8) ![]ZombieListRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = try conn.query( // check-pg-drain: ok — drain called in collectZombieRows
        \\SELECT id::text, name, status, created_at, updated_at
        \\FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC
    , .{workspace_id});
    defer q.deinit();
    return collectZombieRows(alloc, &q);
}

fn collectZombieRows(alloc: std.mem.Allocator, q: anytype) ![]ZombieListRow {
    var rows: std.ArrayList(ZombieListRow) = .{};
    errdefer {
        for (rows.items) |r| {
            alloc.free(r.id);
            alloc.free(r.name);
            alloc.free(r.status);
        }
        rows.deinit(alloc);
    }
    while (try q.*.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const name = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(name);
        const status = try alloc.dupe(u8, try row.get([]const u8, 2));
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .status = status,
            .created_at = try row.get(i64, 3),
            .updated_at = try row.get(i64, 4),
        });
    }
    q.*.drain() catch {};
    return rows.toOwnedSlice(alloc);
}

// ── Delete Zombie ─────────────────────────────────────────────────────

pub fn handleDeleteZombie(ctx: *Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch {
        common.errorResponse(res, .unauthorized, ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED, req_id);
        return;
    };
    _ = principal;

    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7", req_id);
        return;
    }

    const updated = killZombie(ctx.pool, zombie_id) catch |err| {
        log.err("zombie.delete_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, req_id });
        common.internalDbError(res, req_id);
        return;
    };

    if (!updated) {
        common.errorResponse(res, .not_found, ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND, req_id);
        return;
    }

    log.info("zombie.killed id={s}", .{zombie_id});
    res.status = 200;
    res.json(.{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.killed.toSlice() }, .{}) catch {};
}

fn killZombie(pool: *pg.Pool, zombie_id: []const u8) !bool {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    // rows_affected check: if zombie doesn't exist or already killed, return false
    var q = try conn.query( // check-pg-drain: ok — drain called below
        \\UPDATE core.zombies SET status = $1, updated_at = $2
        \\WHERE id = $3::uuid AND status != $1
        \\RETURNING id
    , .{ zombie_config.ZombieStatus.killed.toSlice(), now_ms, zombie_id });
    defer q.deinit();
    const has_row = try q.next() != null;
    q.drain() catch {};
    return has_row;
}

// validateCreateFields is tested via integration tests (DB-backed).
// It requires a fully initialized httpz.Response which cannot be mocked in unit tests.

test "isUniqueViolation always returns false (no SQLSTATE introspection)" {
    try std.testing.expect(!isUniqueViolation(error.PGError));
    try std.testing.expect(!isUniqueViolation(error.OutOfMemory));
}
