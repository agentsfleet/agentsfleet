//! M9_001 §2.0 — Integration Grant CRUD handlers.
//! POST   /v1/zombies/{id}/integration-requests      → handleRequestGrant (zombie auth)
//! GET    /v1/zombies/{id}/integration-grants        → handleListGrants   (workspace auth)
//! DELETE /v1/zombies/{id}/integration-grants/{gid}  → handleRevokeGrant  (workspace auth)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");

const log = std.log.scoped(.integration_grants);

pub const Context = common.Context;

// ── Zombie auth helpers (Path A + Path B) ─────────────────────────────────
// Mirrors execute.zig auth — caller must prove zombie identity.

fn sha256Hex(input: []const u8) [64]u8 {
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(input, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

fn constantTimeEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |x, y| diff |= x ^ y;
    return diff == 0;
}

const Caller = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
};

fn zombieFromSession(alloc: std.mem.Allocator, conn: *pg.Conn, token: []const u8) ?Caller {
    var q = PgQuery.from(conn.query(
        \\SELECT s.zombie_id::text, z.workspace_id::text
        \\FROM core.zombie_sessions s
        \\JOIN core.zombies z ON z.id = s.zombie_id
        \\WHERE s.id = $1::uuid
        \\LIMIT 1
    , .{token}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const zombie_id    = row.get([]u8, 0) catch return null;
    const workspace_id = row.get([]u8, 1) catch return null;
    return .{
        .zombie_id    = alloc.dupe(u8, zombie_id)    catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

fn zombieFromApiKey(alloc: std.mem.Allocator, conn: *pg.Conn, raw_key: []const u8) ?Caller {
    const hex = sha256Hex(raw_key);
    const computed_hash: []const u8 = hex[0..];
    var q = PgQuery.from(conn.query(
        \\SELECT ea.key_hash, ea.zombie_id::text, ea.workspace_id::text
        \\FROM core.external_agents ea
        \\WHERE ea.key_hash = $1
        \\LIMIT 1
    , .{computed_hash}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const stored_hash  = row.get([]u8, 0) catch return null;
    const zombie_id    = row.get([]u8, 1) catch return null;
    const workspace_id = row.get([]u8, 2) catch return null;
    if (!constantTimeEql(computed_hash, stored_hash)) return null;
    return .{
        .zombie_id    = alloc.dupe(u8, zombie_id)    catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

fn authenticateZombie(alloc: std.mem.Allocator, conn: *pg.Conn, req: *httpz.Request) ?Caller {
    const auth_header = req.header("authorization") orelse return null;
    if (std.mem.startsWith(u8, auth_header, "Session "))
        return zombieFromSession(alloc, conn, auth_header["Session ".len..]);
    const bearer_prefix = "Bearer ";
    if (std.mem.startsWith(u8, auth_header, bearer_prefix)) {
        const token = auth_header[bearer_prefix.len..];
        if (std.mem.startsWith(u8, token, "zmb_"))
            return zombieFromApiKey(alloc, conn, token);
    }
    return null;
}

// ── Supported services ────────────────────────────────────────────────────

const SUPPORTED_SERVICES = [_][]const u8{ "slack", "gmail", "agentmail", "discord", "grafana" };
const MAX_REASON_LEN: usize = 512;

fn isSupportedService(service: []const u8) bool {
    for (SUPPORTED_SERVICES) |s| {
        if (std.mem.eql(u8, service, s)) return true;
    }
    return false;
}

// ── handleRequestGrant ─────────────────────────────────────────────────────
// POST /v1/zombies/{zombie_id}/integration-requests
// Zombie authenticates; zombie_id in path must match caller identity.
// Idempotent: returns existing pending grant if one already exists for service.

const RequestGrantBody = struct {
    service: []const u8,
    reason: []const u8,
};

pub fn handleRequestGrant(ctx: *Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const caller = authenticateZombie(alloc, conn, req) orelse {
        common.errorResponse(res, ec.ERR_APIKEY_INVALID,
            "Invalid API key or session. Use: Authorization: Session {uuid} or Authorization: Bearer zmb_xxx", req_id);
        return;
    };

    if (!std.mem.eql(u8, caller.zombie_id, zombie_id)) {
        common.errorResponse(res, ec.ERR_FORBIDDEN,
            "Zombie identity mismatch: authenticated zombie_id does not match path", req_id);
        return;
    }

    const raw_body = req.body() orelse {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(RequestGrantBody, alloc, raw_body, .{}) catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Malformed JSON body", req_id);
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!isSupportedService(body.service)) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST,
            "service must be one of: slack, gmail, agentmail, discord, grafana", req_id);
        return;
    }
    if (body.reason.len == 0 or body.reason.len > MAX_REASON_LEN) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "reason must be 1–512 chars", req_id);
        return;
    }

    // Idempotency: return existing grant if already present for this zombie+service.
    var existing_q = PgQuery.from(conn.query(
        \\SELECT grant_id, status, requested_at FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid AND service = $2
        \\LIMIT 1
    , .{ zombie_id, body.service }) catch {
        common.internalDbError(res, req_id);
        return;
    });
    defer existing_q.deinit();

    if (existing_q.next() catch null) |existing_row| {
        const existing_id  = existing_row.get([]u8, 0) catch "";
        const existing_st  = existing_row.get([]u8, 1) catch "unknown";
        const existing_at  = existing_row.get(i64, 2) catch 0;
        log.info("grant.already_exists zombie_id={s} service={s} status={s}", .{ zombie_id, body.service, existing_st });
        common.writeJson(res, .ok, .{
            .grant_id    = alloc.dupe(u8, existing_id) catch existing_id,
            .zombie_id   = zombie_id,
            .service     = body.service,
            .status      = alloc.dupe(u8, existing_st) catch existing_st,
            .requested_at = existing_at,
            .message     = "Grant already exists for this service.",
        });
        return;
    }

    const grant_id = id_format.generateZombieId(alloc) catch {
        common.internalOperationError(res, "ID generation failed", req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    _ = conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (grant_id, zombie_id, service, scopes, status, requested_at, requested_reason)
        \\VALUES ($1, $2::uuid, $3, ARRAY['*'], 'pending', $4, $5)
    , .{ grant_id, zombie_id, body.service, now_ms, body.reason }) catch {
        common.internalDbError(res, req_id);
        return;
    };

    log.info("grant.requested zombie_id={s} service={s} grant_id={s}", .{ zombie_id, body.service, grant_id });

    common.writeJson(res, .created, .{
        .grant_id     = grant_id,
        .zombie_id    = zombie_id,
        .service      = body.service,
        .status       = "pending",
        .requested_at = now_ms,
        .message      = "Grant request submitted. Awaiting workspace owner approval via Slack, Discord, or dashboard.",
    });
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

    _ = common.authenticate(alloc, req, ctx) catch |err| {
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

    _ = common.authenticate(alloc, req, ctx) catch |err| {
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
