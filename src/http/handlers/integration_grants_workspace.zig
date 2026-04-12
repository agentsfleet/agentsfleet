//! Integration Grant workspace-owner operations.
//! GET    /v1/zombies/{id}/integration-grants        → handleListGrants  (workspace auth)
//! DELETE /v1/zombies/{id}/integration-grants/{gid}  → handleRevokeGrant (workspace auth)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
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

// ── handleListGrants ────────────────────────────────────────────────────────
// GET /v1/zombies/{zombie_id}/integration-grants
// Workspace owner auth (Clerk OIDC or internal API key).

const GrantRow = struct {
    grant_id: []const u8,
    service: []const u8,
    status: []const u8,
    requested_at: i64,
    approved_at: ?i64,
    revoked_at: ?i64,
    reason: []const u8,
};

pub fn handleListGrants(ctx: *Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void {
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

    const zombie_ws_id = getZombieWorkspaceId(conn, alloc, zombie_id) orelse {
        common.errorResponse(res, ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found", req_id);
        return;
    };
    if (!common.authorizeWorkspace(conn, principal, zombie_ws_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    var q = PgQuery.from(conn.query(
        \\SELECT grant_id, service, status, requested_at, approved_at, revoked_at, requested_reason
        \\FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid
        \\ORDER BY requested_at DESC
    , .{zombie_id}) catch {
        common.internalDbError(res, req_id);
        return;
    });
    defer q.deinit();

    var grants: std.ArrayListUnmanaged(GrantRow) = .{};
    while (q.next() catch null) |row| {
        const grant_id    = alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const service     = alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const status      = alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const requested_at = row.get(i64, 3) catch continue;
        const approved_at  = row.get(i64, 4) catch null;
        const revoked_at   = row.get(i64, 5) catch null;
        const reason      = alloc.dupe(u8, row.get([]u8, 6) catch continue) catch continue;
        grants.append(alloc, .{
            .grant_id     = grant_id,
            .service      = service,
            .status       = status,
            .requested_at = requested_at,
            .approved_at  = approved_at,
            .revoked_at   = revoked_at,
            .reason       = reason,
        }) catch {};
    }

    common.writeJson(res, .ok, .{ .grants = grants.items });
}

// ── handleRevokeGrant ───────────────────────────────────────────────────────
// DELETE /v1/zombies/{zombie_id}/integration-grants/{grant_id}
// Workspace owner auth. Sets status = 'revoked', records revoked_at timestamp.

pub fn handleRevokeGrant(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    zombie_id: []const u8,
    grant_id: []const u8,
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

    const zombie_ws_id = getZombieWorkspaceId(conn, alloc, zombie_id) orelse {
        common.errorResponse(res, ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found", req_id);
        return;
    };
    if (!common.authorizeWorkspace(conn, principal, zombie_ws_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE core.integration_grants
        \\SET status = 'revoked', revoked_at = $1
        \\WHERE grant_id = $2 AND zombie_id = $3::uuid AND status != 'revoked'
    , .{ now_ms, grant_id, zombie_id }) catch {
        common.internalDbError(res, req_id);
        return;
    };

    log.info("grant.revoked zombie_id={s} grant_id={s}", .{ zombie_id, grant_id });
    res.status = 204;
    res.body = "";
}
