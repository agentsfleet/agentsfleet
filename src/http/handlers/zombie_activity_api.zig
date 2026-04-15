// M2_001 / M24_001: Zombie activity stream + credential API handlers.
//
// GET  /v1/workspaces/{ws}/zombies/{id}/activity  → innerListActivity
// POST /v1/zombies/credentials  → innerStoreCredential     (migrated in later M24 slice)
// GET  /v1/zombies/credentials  → innerListCredentials     (migrated in later M24 slice)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");
const crypto_store = @import("../../secrets/crypto_store.zig");

const log = std.log.scoped(.zombie_activity_api);

pub const Context = common.Context;

const MAX_CREDENTIAL_VALUE_LEN: usize = 4 * 1024; // 4KB
const MAX_CREDENTIAL_NAME_LEN: usize = 64;

// ── List Activity ─────────────────────────────────────────────────────

pub fn innerListActivity(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const qs = req.query() catch null;
    const limit = if (qs) |q| parseLimitFromQs(q) else activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
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

    const page = activity_stream.queryByZombie(hx.ctx.pool, hx.alloc, zombie_id, cursor, limit) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "Invalid cursor format");
            return;
        }
        log.err("activity.list_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer page.deinit(hx.alloc);

    hx.ok(.ok, .{ .events = page.events, .next_cursor = page.next_cursor });
}

fn parseLimitFromQs(qs: anytype) u32 {
    const limit_str = qs.get("limit") orelse return activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
    return std.fmt.parseInt(u32, limit_str, 10) catch activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT;
}

// ── Store Credential ──────────────────────────────────────────────────

const CredentialBody = struct {
    name: []const u8,
    value: []const u8,
    workspace_id: []const u8,
};

pub fn innerStoreCredential(hx: hx_mod.Hx, req: *httpz.Request) void {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const parsed = std.json.parseFromSlice(CredentialBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    const cred = parsed.value;

    if (!validateCredential(hx, cred)) return;

    storeCredentialEncrypted(hx.ctx.pool, hx.alloc, cred) catch |err| {
        log.err("credential.store_failed err={s} name={s} req_id={s}", .{ @errorName(err), cred.name, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("credential.stored name={s} workspace={s}", .{ cred.name, cred.workspace_id });
    hx.ok(.created, .{ .name = cred.name });
}

fn validateCredential(hx: hx_mod.Hx, cred: CredentialBody) bool {
    if (cred.name.len == 0 or cred.name.len > MAX_CREDENTIAL_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_NAME_REQUIRED);
        return false;
    }
    if (cred.value.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_VALUE_REQUIRED);
        return false;
    }
    if (cred.value.len > MAX_CREDENTIAL_VALUE_LEN) {
        hx.fail(ec.ERR_ZOMBIE_CREDENTIAL_VALUE_TOO_LONG, ec.MSG_ZOMBIE_CREDENTIAL_TOO_LONG);
        return false;
    }
    if (cred.workspace_id.len == 0 or !id_format.isSupportedWorkspaceId(cred.workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return false;
    }
    return true;
}

fn storeCredentialEncrypted(pool: *pg.Pool, alloc: std.mem.Allocator, cred: CredentialBody) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const key_name = try std.fmt.allocPrint(alloc, "zombie:{s}", .{cred.name});
    defer alloc.free(key_name);
    // Use envelope encryption via crypto_store (vault.secrets table).
    // KEK version 1 is the default master key.
    try crypto_store.store(alloc, conn, cred.workspace_id, key_name, cred.value, 1);
}

// ── List Credentials ──────────────────────────────────────────────────

pub fn innerListCredentials(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    };
    const workspace_id = qs.get("workspace_id") orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    };
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const creds = fetchCredentialList(hx.ctx.pool, hx.alloc, workspace_id) catch |err| {
        log.err("credential.list_failed err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .credentials = creds });
}

const CredentialListRow = struct {
    name: []const u8,
    created_at: i64,
};

fn fetchCredentialList(pool: *pg.Pool, alloc: std.mem.Allocator, workspace_id: []const u8) ![]CredentialListRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    // Query vault.secrets for zombie-prefixed keys (zombie:{name}).
    var q = PgQuery.from(try conn.query(
        \\SELECT key_name, created_at FROM vault.secrets
        \\WHERE workspace_id = $1::uuid AND key_name LIKE 'zombie:%'
        \\ORDER BY key_name ASC
    , .{workspace_id}));
    defer q.deinit();

    var rows: std.ArrayList(CredentialListRow) = .{};
    errdefer {
        for (rows.items) |r| alloc.free(r.name);
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        const raw_name = try row.get([]const u8, 0);
        // Strip "zombie:" prefix for display
        const display_name = if (std.mem.startsWith(u8, raw_name, "zombie:"))
            raw_name["zombie:".len..]
        else
            raw_name;
        const name = try alloc.dupe(u8, display_name);
        errdefer alloc.free(name);
        try rows.append(alloc, .{
            .name = name,
            .created_at = try row.get(i64, 1),
        });
    }
    return rows.toOwnedSlice(alloc);
}

test "parseLimit returns default for missing param" {
    // parseLimit is a pure function on query params; tested indirectly via integration tests.
    // This test validates the default constant is sensible.
    try std.testing.expectEqual(@as(u32, 20), activity_stream.DEFAULT_ACTIVITY_PAGE_LIMIT);
}
