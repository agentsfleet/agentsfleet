//! M9_001 §2.0 — Integration Grant request handler (zombie auth).
//! POST /v1/zombies/{id}/integration-requests → handleRequestGrant
//! Workspace-auth operations (list/revoke) are in integration_grants_workspace.zig.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const grant_notifier = @import("../../../zombie/notifications/grant_notifier.zig");
const api_key = @import("../../../auth/api_key.zig");

const log = std.log.scoped(.integration_grants);

pub const Context = common.Context;

// ── Zombie auth helpers (Path A + Path B) ─────────────────────────────────
// Mirrors execute.zig auth — caller must prove zombie identity.

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
    const hex = api_key.sha256Hex(raw_key);
    const computed_hash: []const u8 = hex[0..];
    var q = PgQuery.from(conn.query(
        \\SELECT ea.key_hash, ea.zombie_id::text, ea.workspace_id::text
        \\FROM core.agent_keys ea
        \\WHERE ea.key_hash = $1
        \\LIMIT 1
    , .{computed_hash}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const stored_hash  = row.get([]u8, 0) catch return null;
    const zombie_id    = row.get([]u8, 1) catch return null;
    const workspace_id = row.get([]u8, 2) catch return null;
    if (!api_key.constantTimeEql(computed_hash, stored_hash)) return null;

    // Best-effort: record last use time. Failure is not fatal.
    _ = conn.exec(
        \\UPDATE core.agent_keys SET last_used_at = $1 WHERE key_hash = $2
    , .{ std.time.milliTimestamp(), computed_hash }) catch {};

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

pub fn innerRequestGrant(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const caller = authenticateZombie(hx.alloc, conn, req) orelse {
        hx.fail(ec.ERR_APIKEY_INVALID,
            "Invalid API key or session. Use: Authorization: Session {uuid} or Authorization: Bearer zmb_xxx");
        return;
    };

    if (!std.mem.eql(u8, caller.zombie_id, zombie_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Zombie identity mismatch: authenticated zombie_id does not match path");
        return;
    }

    // M24_001: defence in depth — verify zombie's workspace matches the path workspace_id.
    if (!std.mem.eql(u8, caller.workspace_id, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace identity mismatch: zombie does not belong to path workspace_id");
        return;
    }

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(RequestGrantBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (!isSupportedService(body.service)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "service must be one of: slack, gmail, agentmail, discord, grafana");
        return;
    }
    if (body.reason.len == 0 or body.reason.len > MAX_REASON_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "reason must be 1–512 chars");
        return;
    }

    // Idempotency / re-request logic:
    //   pending | approved → return existing (no new request needed)
    //   revoked | denied   → UPDATE back to pending with new reason (re-request allowed)
    //   no row             → INSERT new grant
    var existing_q = PgQuery.from(conn.query(
        \\SELECT grant_id, status, requested_at FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid AND service = $2
        \\LIMIT 1
    , .{ zombie_id, body.service }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer existing_q.deinit();

    if (existing_q.next() catch null) |existing_row| {
        const existing_id  = existing_row.get([]u8, 0) catch "";
        const existing_st  = existing_row.get([]u8, 1) catch "unknown";
        const existing_at  = existing_row.get(i64, 2) catch 0;

        const is_terminal = std.mem.eql(u8, existing_st, "revoked") or
                            std.mem.eql(u8, existing_st, "denied");
        if (!is_terminal) {
            // pending or approved — idempotent return.
            log.info("grant.already_exists zombie_id={s} service={s} status={s}", .{ zombie_id, body.service, existing_st });
            hx.ok(.ok, .{
                .grant_id     = hx.alloc.dupe(u8, existing_id) catch existing_id,
                .zombie_id    = zombie_id,
                .service      = body.service,
                .status       = hx.alloc.dupe(u8, existing_st) catch existing_st,
                .requested_at = existing_at,
                .message      = "Grant already exists for this service.",
            });
            return;
        }

        // Revoked/denied: re-request — UPDATE back to pending.
        const now_ms_reopen = std.time.milliTimestamp();
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = 'pending', requested_at = $1, requested_reason = $2,
            \\    approved_at = NULL, revoked_at = NULL
            \\WHERE grant_id = $3
        , .{ now_ms_reopen, body.reason, hx.alloc.dupe(u8, existing_id) catch existing_id }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
        log.info("grant.re_requested zombie_id={s} service={s} grant_id={s}", .{ zombie_id, body.service, existing_id });

        // Notify for the re-request using the existing grant_id.
        const existing_grant_id = hx.alloc.dupe(u8, existing_id) catch existing_id;
        var zombie_name_reopen: []const u8 = zombie_id;
        fetch_name_reopen: {
            var nq = PgQuery.from(conn.query(
                \\SELECT name FROM core.zombies WHERE id = $1::uuid LIMIT 1
            , .{zombie_id}) catch break :fetch_name_reopen);
            defer nq.deinit();
            const nrow = nq.next() catch break :fetch_name_reopen orelse break :fetch_name_reopen;
            const raw = nrow.get([]u8, 0) catch break :fetch_name_reopen;
            zombie_name_reopen = hx.alloc.dupe(u8, raw) catch zombie_id;
        }
        grant_notifier.notifyGrantRequest(
            hx.ctx.pool, hx.ctx.queue, hx.alloc,
            zombie_id, caller.workspace_id,
            existing_grant_id, zombie_name_reopen, body.service, body.reason,
        );
        hx.ok(.created, .{
            .grant_id     = existing_grant_id,
            .zombie_id    = zombie_id,
            .service      = body.service,
            .status       = "pending",
            .requested_at = now_ms_reopen,
            .message      = "Grant re-requested. Awaiting workspace owner approval.",
        });
        return;
    }

    const grant_id = id_format.generateZombieId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    _ = conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (grant_id, zombie_id, service, scopes, status, requested_at, requested_reason)
        \\VALUES ($1, $2::uuid, $3, ARRAY['*'], 'pending', $4, $5)
    , .{ grant_id, zombie_id, body.service, now_ms, body.reason }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("grant.requested zombie_id={s} service={s} grant_id={s}", .{ zombie_id, body.service, grant_id });

    // Fetch zombie name for notification; falls back to zombie_id on any error.
    var zombie_name: []const u8 = zombie_id;
    fetch_name: {
        var nq = PgQuery.from(conn.query(
            \\SELECT name FROM core.zombies WHERE id = $1::uuid LIMIT 1
        , .{zombie_id}) catch break :fetch_name);
        defer nq.deinit();
        const nrow = nq.next() catch break :fetch_name orelse break :fetch_name;
        const raw = nrow.get([]u8, 0) catch break :fetch_name;
        zombie_name = hx.alloc.dupe(u8, raw) catch zombie_id;
    }

    grant_notifier.notifyGrantRequest(
        hx.ctx.pool, hx.ctx.queue, hx.alloc,
        zombie_id, caller.workspace_id,
        grant_id, zombie_name, body.service, body.reason,
    );

    hx.ok(.created, .{
        .grant_id     = grant_id,
        .zombie_id    = zombie_id,
        .service      = body.service,
        .status       = "pending",
        .requested_at = now_ms,
        .message      = "Grant request submitted. Awaiting workspace owner approval via Slack, Discord, or dashboard.",
    });
}

