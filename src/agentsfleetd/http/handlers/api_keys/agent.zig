//! Workspace agent-key management.
//! POST   /v1/workspaces/{ws}/agent-keys            → innerCreateAgentKey
//! GET    /v1/workspaces/{ws}/agent-keys            → innerListAgentKeys
//! DELETE /v1/workspaces/{ws}/agent-keys/{agent_key_id} → innerDeleteAgentKey
//!
//! Keys are issued as "zmb_{hex32}" — 32 random bytes as lower-hex prefixed with "zmb_".
//! Only the SHA-256 hash of the key is stored. The raw key is shown once at creation.

const std = @import("std");
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

const log = logging.scoped(.agent_keys);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const KEY_PREFIX = "zmb_";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

const KEY_RANDOM_BYTES: usize = 32;

// ── Key generation ─────────────────────────────────────────────────────────

/// Generate a zmb_ key: "zmb_{64 lower-hex chars}" from 32 random bytes.
/// Returns allocated string owned by alloc. Total length = 4 + 64 = 68 chars.
fn generateApiKey(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [KEY_RANDOM_BYTES]u8 = undefined;
    try constants.secureRandomBytes(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ KEY_PREFIX, hex });
}

// ── innerCreateAgentKey ────────────────────────────────────────────────────
// POST /v1/workspaces/{ws}/agent-keys — bearer policy.
// Returns the raw key exactly once — not stored, cannot be retrieved later.

const CreateAgentBody = struct {
    agent_id: []const u8,
    name: []const u8,
    description: ?[]const u8 = null,
};

const MAX_NAME_LEN: usize = 64;
const MAX_DESC_LEN: usize = 256;

pub fn innerCreateAgentKey(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(CreateAgentBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!id_format.isSupportedAgentId(body.agent_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "agent_id must be a valid UUIDv7");
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

    // Verify agent belongs to this workspace before minting key material.
    {
        var agent_q = PgQuery.from(conn.query(
            \\SELECT 1 FROM core.agents WHERE id = $1::uuid AND workspace_id = $2::uuid LIMIT 1
        , .{ body.agent_id, workspace_id }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        });
        defer agent_q.deinit();
        const agent_row = agent_q.next() catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
        if (agent_row == null) {
            hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, "Agent not found in this workspace");
            return;
        }
    }

    const raw_key = generateApiKey(hx.alloc) catch {
        common.internalOperationError(hx.res, "Key generation failed", hx.req_id);
        return;
    };
    const key_hash_arr = api_key.sha256Hex(raw_key);
    const key_hash: []const u8 = key_hash_arr[0..];

    const agent_key_id = id_format.generateAgentId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();
    const desc = body.description orelse "";

    _ = conn.exec(
        \\INSERT INTO core.agent_keys
        \\  (uid, agent_key_id, workspace_id, agent_id, name, description, key_hash, created_at)
        \\VALUES ($1::uuid, $1, $2::uuid, $3::uuid, $4, $5, $6, $7)
    , .{ agent_key_id, workspace_id, body.agent_id, body.name, desc, key_hash, now_ms }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("created", .{
        .agent_key_id = agent_key_id,
        .workspace_id = workspace_id,
        .agent_id = body.agent_id,
    });

    // Return the raw key once — callers must store it; it cannot be retrieved again.
    hx.ok(.created, .{
        .agent_key_id = agent_key_id,
        .workspace_id = workspace_id,
        .agent_id = body.agent_id,
        .name = body.name,
        .key = raw_key, // shown once — store securely
        .created_at = now_ms,
        .message = "Store this key securely. It will not be shown again.",
    });
}

// ── innerListAgentKeys ─────────────────────────────────────────────────────
// GET /v1/workspaces/{ws}/agent-keys — bearer policy.
// Returns agent metadata — never returns key_hash.

const AgentRow = struct {
    agent_key_id: []const u8,
    agent_id: []const u8,
    name: []const u8,
    description: []const u8,
    created_at: i64,
    last_used_at: ?i64,
};

pub fn innerListAgentKeys(hx: Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    var q = PgQuery.from(conn.query(
        \\SELECT agent_key_id, agent_id::text, name, description, created_at, last_used_at
        \\FROM core.agent_keys
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC
    , .{workspace_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    var agents: std.ArrayList(AgentRow) = .empty;
    while (q.next() catch null) |row| {
        const agent_key_id = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const agent_id = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const name = hx.alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const description = hx.alloc.dupe(u8, row.get([]u8, 3) catch continue) catch continue;
        const created_at = row.get(i64, 4) catch continue;
        const last_used = row.get(i64, 5) catch null;
        agents.append(hx.alloc, .{
            .agent_key_id = agent_key_id,
            .agent_id = agent_id,
            .name = name,
            .description = description,
            .created_at = created_at,
            .last_used_at = last_used,
        }) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
    }

    hx.ok(.ok, .{ .items = agents.items, .total = agents.items.len });
}

// ── innerDeleteAgentKey ────────────────────────────────────────────────────
// DELETE /v1/workspaces/{ws}/agent-keys/{agent_key_id} — bearer policy.

pub fn innerDeleteAgentKey(hx: Hx, workspace_id: []const u8, agent_key_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    var del_q = PgQuery.from(conn.query(
        \\DELETE FROM core.agent_keys
        \\WHERE agent_key_id = $1 AND workspace_id = $2::uuid
        \\RETURNING agent_key_id
    , .{ agent_key_id, workspace_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer del_q.deinit();

    const deleted = del_q.next() catch null;
    if (deleted == null) {
        hx.fail(ec.ERR_AGENT_NOT_FOUND, "Agent key not found");
        return;
    }

    log.info("deleted", .{ .agent_key_id = agent_key_id, .workspace_id = workspace_id });
    hx.noContent();
}
