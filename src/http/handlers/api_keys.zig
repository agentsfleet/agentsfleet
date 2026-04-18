//! Tenant API key CRUD (/v1/api-keys).
//! Every SQL query filters WHERE tenant_id = principal.tenant_id; env-var
//! bootstrap principals (tenant_id == null) are rejected with 403.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const api_key = @import("../../auth/api_key.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const tenant_api_key = @import("../../auth/middleware/tenant_api_key.zig");
const api_keys_list = @import("api_keys_list.zig");

pub const innerListApiKeys = api_keys_list.innerListApiKeys;
pub const sortClauseFor = api_keys_list.sortClauseFor;

const Hx = hx_mod.Hx;
const log = std.log.scoped(.api_keys);

pub const KEY_PREFIX = tenant_api_key.TENANT_KEY_PREFIX; // "zmb_t_"
pub const KEY_RANDOM_BYTES: usize = 32;
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_DESC_LEN: usize = 256;

fn requireTenantId(hx: Hx) ?[]const u8 {
    const tid = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required; bootstrap principals cannot manage tenant API keys");
        return null;
    };
    return tid;
}

fn requireUserId(hx: Hx) ?[]const u8 {
    const uid = hx.principal.user_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "User context required; bootstrap principals cannot mint tenant API keys");
        return null;
    };
    return uid;
}

pub fn generateRawKey(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [KEY_RANDOM_BYTES]u8 = undefined;
    std.crypto.random.bytes(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ KEY_PREFIX, hex });
}

pub fn isValidKeyName(name: []const u8) bool {
    if (name.len == 0 or name.len > MAX_NAME_LEN) return false;
    for (name) |c| {
        const ok = (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or
            (c >= '0' and c <= '9') or c == '-' or c == '_';
        if (!ok) return false;
    }
    return true;
}

const CreateBody = struct {
    key_name: []const u8,
    description: ?[]const u8 = null,
};

pub fn innerCreateApiKey(hx: Hx, req: *httpz.Request) void {
    const tenant_id = requireTenantId(hx) orelse return;
    const user_id = requireUserId(hx) orelse return;

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(CreateBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!isValidKeyName(body.key_name)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "key_name must be 1-64 chars, alphanumeric + hyphen + underscore");
        return;
    }
    if (body.description) |d| if (d.len > MAX_DESC_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "description must be <=256 chars");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    performCreate(hx, conn, tenant_id, user_id, body) catch |err| switch (err) {
        error.NameTaken => hx.fail(ec.ERR_APIKEY_NAME_TAKEN, "Key name already exists in this tenant"),
        error.DbError => common.internalDbError(hx.res, hx.req_id),
        error.OperationError => common.internalOperationError(hx.res, "API key mint failed", hx.req_id),
    };
}

const CreateError = error{ NameTaken, DbError, OperationError };

fn performCreate(
    hx: Hx,
    conn: *pg.Conn,
    tenant_id: []const u8,
    user_id: []const u8,
    body: CreateBody,
) CreateError!void {
    var check_q = PgQuery.from(conn.query(
        \\SELECT 1 FROM core.api_keys WHERE tenant_id = $1::uuid AND key_name = $2 LIMIT 1
    , .{ tenant_id, body.key_name }) catch return error.DbError);
    defer check_q.deinit();
    if ((check_q.next() catch return error.DbError) != null) return error.NameTaken;

    const raw_key = generateRawKey(hx.alloc) catch return error.OperationError;
    const key_hash = api_key.sha256Hex(raw_key);
    const id = id_format.allocUuidV7(hx.alloc) catch return error.OperationError;
    const now_ms = std.time.milliTimestamp();

    _ = conn.exec(
        \\INSERT INTO core.api_keys (id, tenant_id, key_name, key_hash, created_by, active)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, TRUE)
    , .{ id, tenant_id, body.key_name, key_hash[0..], user_id }) catch return error.DbError;

    log.info("api_key.created tenant_id={s} actor_user_id={s} api_key_id={s} key_name={s}", .{
        tenant_id, user_id, id, body.key_name,
    });

    hx.ok(.created, .{
        .id = id,
        .key_name = body.key_name,
        .key = raw_key,
        .created_at = now_ms,
    });
}

const PatchBody = struct { active: bool };

pub fn innerPatchApiKey(hx: Hx, req: *httpz.Request, key_id: []const u8) void {
    const tenant_id = requireTenantId(hx) orelse return;
    const user_id = hx.principal.user_id orelse "";
    if (!id_format.isUuidV7(key_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "id must be a valid UUIDv7");
        return;
    }

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "PATCH body must be {\"active\": false}");
        return;
    };
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "PATCH body must be {\"active\": false}");
        return;
    };
    defer parsed.deinit();
    if (parsed.value.active) {
        hx.fail(ec.ERR_APIKEY_READONLY_FIELD, "active cannot be set to true; mint a new key instead");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    applyRevoke(hx, conn, tenant_id, user_id, key_id);
}

fn applyRevoke(hx: Hx, conn: *pg.Conn, tenant_id: []const u8, user_id: []const u8, key_id: []const u8) void {
    var q = PgQuery.from(conn.query(
        \\UPDATE core.api_keys
        \\SET active = FALSE, revoked_at = now(), updated_at = now()
        \\WHERE id = $1::uuid AND tenant_id = $2::uuid AND active = TRUE
        \\RETURNING id::text, (EXTRACT(EPOCH FROM revoked_at) * 1000)::bigint
    , .{ key_id, tenant_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    const row = q.next() catch null;
    if (row == null) {
        reportRevokeFailure(hx, conn, tenant_id, key_id);
        return;
    }
    const id = hx.alloc.dupe(u8, row.?.get([]u8, 0) catch "") catch "";
    const revoked_at = row.?.get(i64, 1) catch 0;

    log.info("api_key.revoked tenant_id={s} actor_user_id={s} api_key_id={s}", .{
        tenant_id, user_id, id,
    });

    hx.ok(.ok, .{ .id = id, .active = false, .revoked_at = revoked_at });
}

fn reportRevokeFailure(hx: Hx, conn: *pg.Conn, tenant_id: []const u8, key_id: []const u8) void {
    var q = PgQuery.from(conn.query(
        \\SELECT active FROM core.api_keys WHERE id = $1::uuid AND tenant_id = $2::uuid LIMIT 1
    , .{ key_id, tenant_id }) catch {
        hx.fail(ec.ERR_APIKEY_NOT_FOUND, "API key not found");
        return;
    });
    defer q.deinit();
    const row = q.next() catch null;
    if (row == null) {
        hx.fail(ec.ERR_APIKEY_NOT_FOUND, "API key not found");
        return;
    }
    hx.fail(ec.ERR_APIKEY_ALREADY_REVOKED, "API key is already revoked");
}

pub fn innerDeleteApiKey(hx: Hx, key_id: []const u8) void {
    const tenant_id = requireTenantId(hx) orelse return;
    const user_id = hx.principal.user_id orelse "";
    if (!id_format.isUuidV7(key_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "id must be a valid UUIDv7");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var q = PgQuery.from(conn.query(
        \\DELETE FROM core.api_keys
        \\WHERE id = $1::uuid AND tenant_id = $2::uuid AND active = FALSE
        \\RETURNING id::text
    , .{ key_id, tenant_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    if ((q.next() catch null) == null) {
        reportDeleteFailure(hx, tenant_id, key_id);
        return;
    }

    log.info("api_key.deleted tenant_id={s} actor_user_id={s} api_key_id={s}", .{
        tenant_id, user_id, key_id,
    });
    hx.noContent();
}

fn reportDeleteFailure(hx: Hx, tenant_id: []const u8, key_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        hx.fail(ec.ERR_APIKEY_NOT_FOUND, "API key not found");
        return;
    };
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(conn.query(
        \\SELECT active FROM core.api_keys WHERE id = $1::uuid AND tenant_id = $2::uuid LIMIT 1
    , .{ key_id, tenant_id }) catch {
        hx.fail(ec.ERR_APIKEY_NOT_FOUND, "API key not found");
        return;
    });
    defer q.deinit();
    const row = q.next() catch null;
    if (row == null) {
        hx.fail(ec.ERR_APIKEY_NOT_FOUND, "API key not found");
        return;
    }
    hx.fail(ec.ERR_APIKEY_MUST_REVOKE_FIRST, "Active key must be revoked before deletion");
}

test {
    _ = @import("api_keys_test.zig");
}
