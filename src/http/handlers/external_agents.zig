//! M9_001 §3.0 — External Agent Key management.
//! POST   /v1/workspaces/{ws}/external-agents           → handleCreateExternalAgent
//! GET    /v1/workspaces/{ws}/external-agents           → handleListExternalAgents
//! DELETE /v1/workspaces/{ws}/external-agents/{agent_id} → handleDeleteExternalAgent
//!
//! Keys are issued as "zmb_{hex32}" — 32 random bytes as lower-hex prefixed with "zmb_".
//! Only the SHA-256 hash of the key is stored. The raw key is shown once at creation.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const api_key = @import("../../auth/api_key.zig");

const log = std.log.scoped(.external_agents);

pub const Context = common.Context;

const KEY_PREFIX = "zmb_";
const KEY_RANDOM_BYTES: usize = 32;

// ── Key generation ─────────────────────────────────────────────────────────

/// Generate a zmb_ key: "zmb_{64 lower-hex chars}" from 32 random bytes.
/// Returns allocated string owned by alloc. Total length = 4 + 64 = 68 chars.
fn generateApiKey(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [KEY_RANDOM_BYTES]u8 = undefined;
    std.crypto.random.bytes(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ KEY_PREFIX, hex });
}

// ── handleCreateExternalAgent ───────────────────────────────────────────────
// POST /v1/workspaces/{ws}/external-agents
// Workspace owner auth (Clerk OIDC or internal API key).
// Returns the raw key exactly once — not stored, cannot be retrieved later.

const CreateAgentBody = struct {
    zombie_id: []const u8,
    name: []const u8,
    description: ?[]const u8 = null,
};

const MAX_NAME_LEN: usize = 64;
const MAX_DESC_LEN: usize = 256;

pub fn handleCreateExternalAgent(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        switch (err) {
            error.Unauthorized => common.errorResponse(res, ec.ERR_UNAUTHORIZED, "Authentication required", req_id),
            error.TokenExpired => common.errorResponse(res, ec.ERR_TOKEN_EXPIRED, "Token expired", req_id),
            error.AuthServiceUnavailable => common.errorResponse(res, ec.ERR_AUTH_UNAVAILABLE, "Auth service unavailable", req_id),
            error.UnsupportedRole => common.errorResponse(res, ec.ERR_UNSUPPORTED_ROLE, "Unsupported role", req_id),
        }
        return;
    };

    const raw_body = req.body() orelse {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(CreateAgentBody, alloc, raw_body, .{}) catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Malformed JSON body", req_id);
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!id_format.isSupportedAgentId(body.zombie_id)) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7", req_id);
        return;
    }
    if (body.name.len == 0 or body.name.len > MAX_NAME_LEN) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "name must be 1–64 chars", req_id);
        return;
    }
    if (body.description) |d| {
        if (d.len > MAX_DESC_LEN) {
            common.errorResponse(res, ec.ERR_INVALID_REQUEST, "description must be ≤256 chars", req_id);
            return;
        }
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    // Verify zombie belongs to this workspace.
    var zombie_q = PgQuery.from(conn.query(
        \\SELECT id FROM core.zombies WHERE id = $1::uuid AND workspace_id = $2::uuid LIMIT 1
    , .{ body.zombie_id, workspace_id }) catch {
        common.internalDbError(res, req_id);
        return;
    });
    defer zombie_q.deinit();
    const zombie_row = zombie_q.next() catch null;
    if (zombie_row == null) {
        common.errorResponse(res, ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found in this workspace", req_id);
        return;
    }

    const raw_key = generateApiKey(alloc) catch {
        common.internalOperationError(res, "Key generation failed", req_id);
        return;
    };
    const key_hash_arr = api_key.sha256Hex(raw_key);
    const key_hash: []const u8 = key_hash_arr[0..];

    const agent_id = id_format.generateZombieId(alloc) catch {
        common.internalOperationError(res, "ID generation failed", req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();
    const desc = body.description orelse "";

    _ = conn.exec(
        \\INSERT INTO core.external_agents
        \\  (agent_id, workspace_id, zombie_id, name, description, key_hash, created_at)
        \\VALUES ($1, $2::uuid, $3::uuid, $4, $5, $6, $7)
    , .{ agent_id, workspace_id, body.zombie_id, body.name, desc, key_hash, now_ms }) catch {
        common.internalDbError(res, req_id);
        return;
    };

    log.info("external_agent.created agent_id={s} workspace_id={s} zombie_id={s}", .{
        agent_id, workspace_id, body.zombie_id,
    });

    // Return the raw key once — callers must store it; it cannot be retrieved again.
    common.writeJson(res, .created, .{
        .agent_id    = agent_id,
        .workspace_id = workspace_id,
        .zombie_id   = body.zombie_id,
        .name        = body.name,
        .key         = raw_key, // shown once — store securely
        .created_at  = now_ms,
        .message     = "Store this key securely. It will not be shown again.",
    });
}

// ── handleListExternalAgents ────────────────────────────────────────────────
// GET /v1/workspaces/{ws}/external-agents
// Workspace owner auth. Returns agent metadata — never returns key_hash.

const AgentRow = struct {
    agent_id: []const u8,
    zombie_id: []const u8,
    name: []const u8,
    description: []const u8,
    created_at: i64,
    last_used_at: ?i64,
};

pub fn handleListExternalAgents(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        switch (err) {
            error.Unauthorized => common.errorResponse(res, ec.ERR_UNAUTHORIZED, "Authentication required", req_id),
            error.TokenExpired => common.errorResponse(res, ec.ERR_TOKEN_EXPIRED, "Token expired", req_id),
            error.AuthServiceUnavailable => common.errorResponse(res, ec.ERR_AUTH_UNAVAILABLE, "Auth service unavailable", req_id),
            error.UnsupportedRole => common.errorResponse(res, ec.ERR_UNSUPPORTED_ROLE, "Unsupported role", req_id),
        }
        return;
    };

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    var q = PgQuery.from(conn.query(
        \\SELECT agent_id, zombie_id::text, name, description, created_at, last_used_at
        \\FROM core.external_agents
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC
    , .{workspace_id}) catch {
        common.internalDbError(res, req_id);
        return;
    });
    defer q.deinit();

    var agents: std.ArrayListUnmanaged(AgentRow) = .{};
    while (q.next() catch null) |row| {
        const agent_id    = alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const zombie_id   = alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const name        = alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const description = alloc.dupe(u8, row.get([]u8, 3) catch continue) catch continue;
        const created_at  = row.get(i64, 4) catch continue;
        const last_used   = row.get(i64, 5) catch null;
        agents.append(alloc, .{
            .agent_id    = agent_id,
            .zombie_id   = zombie_id,
            .name        = name,
            .description = description,
            .created_at  = created_at,
            .last_used_at = last_used,
        }) catch {};
    }

    common.writeJson(res, .ok, .{ .agents = agents.items });
}

// ── handleDeleteExternalAgent ───────────────────────────────────────────────
// DELETE /v1/workspaces/{ws}/external-agents/{agent_id}
// Workspace owner auth. Hard-deletes the agent row — key immediately invalidated.

pub fn handleDeleteExternalAgent(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
    agent_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        switch (err) {
            error.Unauthorized => common.errorResponse(res, ec.ERR_UNAUTHORIZED, "Authentication required", req_id),
            error.TokenExpired => common.errorResponse(res, ec.ERR_TOKEN_EXPIRED, "Token expired", req_id),
            error.AuthServiceUnavailable => common.errorResponse(res, ec.ERR_AUTH_UNAVAILABLE, "Auth service unavailable", req_id),
            error.UnsupportedRole => common.errorResponse(res, ec.ERR_UNSUPPORTED_ROLE, "Unsupported role", req_id),
        }
        return;
    };

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    var del_q = PgQuery.from(conn.query(
        \\DELETE FROM core.external_agents
        \\WHERE agent_id = $1 AND workspace_id = $2::uuid
        \\RETURNING agent_id
    , .{ agent_id, workspace_id }) catch {
        common.internalDbError(res, req_id);
        return;
    });
    defer del_q.deinit();

    const deleted = del_q.next() catch null;
    if (deleted == null) {
        common.errorResponse(res, ec.ERR_AGENT_NOT_FOUND, "External agent not found", req_id);
        return;
    }

    log.info("external_agent.deleted agent_id={s} workspace_id={s}", .{ agent_id, workspace_id });
    res.status = 204;
    res.body = "";
}
