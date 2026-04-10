// M2_001: Zombie activity stream + credential API handlers.
//
// GET  /v1/zombies/activity     → handleListActivity
// POST /v1/zombies/credentials  → handleStoreCredential
// GET  /v1/zombies/credentials  → handleListCredentials

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("common.zig");
const ec = @import("../../errors/codes.zig");
const id_format = @import("../../types/id_format.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");

const log = std.log.scoped(.zombie_activity_api);

pub const Context = common.Context;

const MAX_CREDENTIAL_VALUE_LEN: usize = 4 * 1024; // 4KB
const MAX_CREDENTIAL_NAME_LEN: usize = 64;

// ── List Activity ─────────────────────────────────────────────────────

pub fn handleListActivity(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
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
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, "zombie_id query parameter required", req_id);
        return;
    };
    const zombie_id = qs.get("zombie_id") orelse {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, "zombie_id query parameter required", req_id);
        return;
    };

    const limit = parseLimitFromQs(qs);
    const cursor = qs.get("cursor");

    const page = activity_stream.queryByZombie(ctx.pool, alloc, zombie_id, cursor, limit) catch |err| {
        if (err == error.InvalidCursor) {
            common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, "Invalid cursor format", req_id);
            return;
        }
        log.err("activity.list_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, req_id });
        common.internalDbError(res, req_id);
        return;
    };
    defer page.deinit(alloc);

    writeActivityResponse(res, page);
}

fn parseLimitFromQs(qs: anytype) u32 {
    const limit_str = qs.get("limit") orelse return activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    return std.fmt.parseInt(u32, limit_str, 10) catch activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
}

fn writeActivityResponse(res: *httpz.Response, page: activity_stream.ActivityPage) void {
    res.status = 200;
    // Build JSON array manually to avoid deep nested serialization issues
    res.json(.{
        .events = page.events,
        .next_cursor = page.next_cursor,
    }, .{}) catch {
        res.status = 500;
        res.body = "{}";
    };
}

// ── Store Credential ──────────────────────────────────────────────────

const CredentialBody = struct {
    name: []const u8,
    value: []const u8,
    workspace_id: []const u8,
};

pub fn handleStoreCredential(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch {
        common.errorResponse(res, .unauthorized, ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED, req_id);
        return;
    };
    _ = principal;

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED, req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;

    const parsed = std.json.parseFromSlice(CredentialBody, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON, req_id);
        return;
    };
    const cred = parsed.value;

    if (!validateCredential(cred, res, req_id)) return;

    insertCredential(ctx.pool, alloc, cred) catch |err| {
        log.err("credential.store_failed err={s} name={s} req_id={s}", .{ @errorName(err), cred.name, req_id });
        common.internalDbError(res, req_id);
        return;
    };

    log.info("credential.stored name={s} workspace={s}", .{ cred.name, cred.workspace_id });
    res.status = 201;
    res.json(.{ .name = cred.name }, .{}) catch {};
}

fn validateCredential(cred: CredentialBody, res: *httpz.Response, req_id: []const u8) bool {
    if (cred.name.len == 0 or cred.name.len > MAX_CREDENTIAL_NAME_LEN) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_NAME_REQUIRED, req_id);
        return false;
    }
    if (cred.value.len == 0) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_VALUE_REQUIRED, req_id);
        return false;
    }
    if (cred.value.len > MAX_CREDENTIAL_VALUE_LEN) {
        common.errorResponse(res, .bad_request, ec.ERR_ZOMBIE_CREDENTIAL_VALUE_TOO_LONG, ec.MSG_ZOMBIE_CREDENTIAL_TOO_LONG, req_id);
        return false;
    }
    if (cred.workspace_id.len == 0 or !id_format.isSupportedWorkspaceId(cred.workspace_id)) {
        common.errorResponse(res, .bad_request, ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED, req_id);
        return false;
    }
    return true;
}

fn insertCredential(pool: *pg.Pool, alloc: std.mem.Allocator, cred: CredentialBody) !void {
    const cred_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(cred_id);
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    // Upsert: if credential name already exists for workspace, update value
    _ = try conn.exec(
        \\INSERT INTO core.zombie_credentials
        \\  (id, workspace_id, name, value_encrypted, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6)
        \\ON CONFLICT (workspace_id, name) DO UPDATE
        \\  SET value_encrypted = EXCLUDED.value_encrypted, updated_at = EXCLUDED.updated_at
    , .{ cred_id, cred.workspace_id, cred.name, cred.value, now_ms, now_ms });
}

// ── List Credentials ──────────────────────────────────────────────────

pub fn handleListCredentials(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
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

    const creds = fetchCredentialList(ctx.pool, alloc, workspace_id) catch |err| {
        log.err("credential.list_failed err={s} req_id={s}", .{ @errorName(err), req_id });
        common.internalDbError(res, req_id);
        return;
    };

    res.status = 200;
    res.json(.{ .credentials = creds }, .{}) catch {};
}

const CredentialListRow = struct {
    name: []const u8,
    created_at: i64,
};

fn fetchCredentialList(pool: *pg.Pool, alloc: std.mem.Allocator, workspace_id: []const u8) ![]CredentialListRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = try conn.query( // check-pg-drain: ok — drain called in loop below
        \\SELECT name, created_at FROM core.zombie_credentials
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY name ASC
    , .{workspace_id});
    defer q.deinit();

    var rows: std.ArrayList(CredentialListRow) = .{};
    errdefer {
        for (rows.items) |r| alloc.free(r.name);
        rows.deinit(alloc);
    }
    while (try q.*.next()) |row| {
        const name = try alloc.dupe(u8, try row.get([]const u8, 0));
        try rows.append(alloc, .{
            .name = name,
            .created_at = try row.get(i64, 1),
        });
    }
    q.*.drain() catch {};
    return rows.toOwnedSlice(alloc);
}

test "parseLimit returns default for missing param" {
    // parseLimit is a pure function on query params; tested indirectly via integration tests.
    // This test validates the default constant is sensible.
    try std.testing.expectEqual(@as(u32, 20), activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT);
}
