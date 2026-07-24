//! Workspace fleet-key management.
//! POST   /v1/workspaces/{ws}/fleet-keys            → innerCreateFleetKey
//! GET    /v1/workspaces/{ws}/fleet-keys            → innerListFleetKeys
//! DELETE /v1/workspaces/{ws}/fleet-keys/{fleet_key_id} → innerDeleteFleetKey
//!
//! Keys are issued as "agt_a{hex32}" — 32 random bytes as lower-hex prefixed with "agt_a".
//! Only the SHA-256 hash of the key is stored. The raw key is shown once at creation.

const std = @import("std");
const sql = @import("sql.zig");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const api_key = @import("../../../auth/api_key.zig");

const log = logging.scoped(.fleet_keys);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const KEY_PREFIX = api_key.KEY_PREFIX;
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

const KEY_RANDOM_BYTES: usize = 32;

// ── Key generation ─────────────────────────────────────────────────────────

/// Generate a agt_a key: "agt_a{64 lower-hex chars}" from 32 random bytes.
/// Returns allocated string owned by alloc. Total length = 4 + 64 = 68 chars.
fn generateApiKey(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [KEY_RANDOM_BYTES]u8 = undefined;
    try constants.secureRandomBytes(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ KEY_PREFIX, hex });
}

// ── innerCreateFleetKey ────────────────────────────────────────────────────
// POST /v1/workspaces/{ws}/fleet-keys — bearer policy.
// Returns the raw key exactly once — not stored, cannot be retrieved later.

const CreateFleetBody = struct {
    fleet_id: []const u8,
    name: []const u8,
    description: ?[]const u8 = null,
};

const MAX_NAME_LEN: usize = 64;
const MAX_DESC_LEN: usize = 256;

pub fn innerCreateFleetKey(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(CreateFleetBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!id_format.isSupportedFleetId(body.fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a valid UUIDv7");
        return;
    }
    if (body.name.len == 0 or body.name.len > MAX_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "name must be 1–64 chars");
        return;
    }
    if (body.description) |d| {
        if (d.len > MAX_DESC_LEN) {
            hx.fail(ec.ERR_INVALID_REQUEST, "description must be ≤256 chars");
            return;
        }
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    // Verify fleet belongs to this workspace before minting key material.
    {
        var fleet_q = PgQuery.from(conn.query(sql.SELECT_FLEET_IN_WORKSPACE, .{ body.fleet_id, workspace_id }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        });
        defer fleet_q.deinit();
        const fleet_row = fleet_q.next() catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
        if (fleet_row == null) {
            hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, "Fleet not found in this workspace");
            return;
        }
    }

    const raw_key = generateApiKey(hx.alloc) catch {
        common.internalOperationError(hx.res, "Key generation failed", hx.req_id);
        return;
    };
    const key_hash_arr = api_key.sha256Hex(raw_key);
    const key_hash: []const u8 = key_hash_arr[0..];

    const fleet_key_id = id_format.generateFleetId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();
    const desc = body.description orelse "";

    _ = conn.exec(sql.INSERT_FLEET_KEY, .{ fleet_key_id, workspace_id, body.fleet_id, body.name, desc, key_hash, now_ms }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.debug("created", .{
        .fleet_key_id = fleet_key_id,
        .workspace_id = workspace_id,
        .fleet_id = body.fleet_id,
    });

    // Return the raw key once — callers must store it; it cannot be retrieved again.
    hx.okSensitive(.created, .{
        .fleet_key_id = fleet_key_id,
        .workspace_id = workspace_id,
        .fleet_id = body.fleet_id,
        .name = body.name,
        .key = raw_key, // shown once — store securely
        .created_at = now_ms,
        .message = "Store this key securely. It will not be shown again.",
    });
}

// ── innerListFleetKeys ─────────────────────────────────────────────────────
// GET /v1/workspaces/{ws}/fleet-keys — bearer policy.
// Returns fleet metadata — never returns key_hash.

const FleetRow = struct {
    fleet_key_id: []const u8,
    fleet_id: []const u8,
    name: []const u8,
    description: []const u8,
    created_at: i64,
    last_used_at: ?i64,
};

pub fn innerListFleetKeys(hx: Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    var q = PgQuery.from(conn.query(sql.SELECT_FLEET_KEYS_FOR_WORKSPACE, .{workspace_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    var fleets: std.ArrayList(FleetRow) = .empty;
    while (q.next() catch null) |row| {
        const fleet_key_id = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const fleet_id = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const name = hx.alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const description = hx.alloc.dupe(u8, row.get([]u8, 3) catch continue) catch continue;
        const created_at = row.get(i64, 4) catch continue;
        const last_used = row.get(i64, 5) catch null;
        fleets.append(hx.alloc, .{
            .fleet_key_id = fleet_key_id,
            .fleet_id = fleet_id,
            .name = name,
            .description = description,
            .created_at = created_at,
            .last_used_at = last_used,
        }) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    }

    hx.ok(.ok, .{ .items = fleets.items, .total = fleets.items.len });
}

// ── innerDeleteFleetKey ────────────────────────────────────────────────────
// DELETE /v1/workspaces/{ws}/fleet-keys/{fleet_key_id} — bearer policy.

pub fn innerDeleteFleetKey(hx: Hx, workspace_id: []const u8, fleet_key_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    var del_q = PgQuery.from(conn.query(sql.DELETE_FLEET_KEY, .{ fleet_key_id, workspace_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer del_q.deinit();

    const deleted = del_q.next() catch null;
    if (deleted == null) {
        hx.fail(ec.ERR_FLEET_KEY_NOT_FOUND, "Fleet key not found");
        return;
    }

    log.debug("deleted", .{ .fleet_key_id = fleet_key_id, .workspace_id = workspace_id });
    hx.noContent();
}
