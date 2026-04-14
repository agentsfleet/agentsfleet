//! Integration Grant workspace-owner operations.
//! GET    /v1/zombies/{id}/integration-grants        → innerListGrants  (bearer policy)
//! DELETE /v1/zombies/{id}/integration-grants/{gid}  → innerRevokeGrant (bearer policy)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");

const log = std.log.scoped(.integration_grants);

pub const Context = common.Context;

// ── Workspace ownership check ─────────────────────────────────────────────

/// Look up the workspace_id for a zombie. Returns null if not found.
pub fn getZombieWorkspaceId(conn: *pg.Conn, alloc: std.mem.Allocator, zombie_id: []const u8) ?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid LIMIT 1
    , .{zombie_id}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const ws = row.get([]u8, 0) catch return null;
    return alloc.dupe(u8, ws) catch null;
}

// ── innerListGrants ────────────────────────────────────────────────────────
// GET /v1/zombies/{zombie_id}/integration-grants
// bearer policy — principal set by middleware.

const GrantRow = struct {
    grant_id: []const u8,
    service: []const u8,
    status: []const u8,
    requested_at: i64,
    approved_at: ?i64,
    revoked_at: ?i64,
    reason: []const u8,
};

pub fn innerListGrants(hx: hx_mod.Hx, zombie_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const zombie_ws_id = getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    };
    if (!common.authorizeWorkspace(conn, hx.principal, zombie_ws_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    var q = PgQuery.from(conn.query(
        \\SELECT grant_id, service, status, requested_at, approved_at, revoked_at, requested_reason
        \\FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid
        \\ORDER BY requested_at DESC
    , .{zombie_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    var grants: std.ArrayListUnmanaged(GrantRow) = .{};
    while (q.next() catch null) |row| {
        const grant_id    = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const service     = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const status      = hx.alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const requested_at = row.get(i64, 3) catch continue;
        const approved_at  = row.get(i64, 4) catch null;
        const revoked_at   = row.get(i64, 5) catch null;
        const reason      = hx.alloc.dupe(u8, row.get([]u8, 6) catch continue) catch continue;
        grants.append(hx.alloc, .{
            .grant_id     = grant_id,
            .service      = service,
            .status       = status,
            .requested_at = requested_at,
            .approved_at  = approved_at,
            .revoked_at   = revoked_at,
            .reason       = reason,
        }) catch {};
    }

    hx.ok(.ok, .{ .grants = grants.items });
}

// ── innerRevokeGrant ───────────────────────────────────────────────────────
// DELETE /v1/zombies/{zombie_id}/integration-grants/{grant_id}
// bearer policy — principal set by middleware.

pub fn innerRevokeGrant(hx: hx_mod.Hx, zombie_id: []const u8, grant_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const zombie_ws_id = getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    };
    if (!common.authorizeWorkspace(conn, hx.principal, zombie_ws_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const now_ms = std.time.milliTimestamp();
    var rev_q = PgQuery.from(conn.query(
        \\UPDATE core.integration_grants
        \\SET status = 'revoked', revoked_at = $1
        \\WHERE grant_id = $2 AND zombie_id = $3::uuid AND status != 'revoked'
        \\RETURNING grant_id
    , .{ now_ms, grant_id, zombie_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer rev_q.deinit();

    const revoked = rev_q.next() catch null;
    if (revoked == null) {
        hx.fail(ec.ERR_GRANT_NOT_FOUND, "Grant not found or already revoked");
        return;
    }

    log.info("grant.revoked zombie_id={s} grant_id={s}", .{ zombie_id, grant_id });
    hx.res.status = 204;
    hx.res.body = "";
}
