//! Integration Grant request handler (fleet auth).
//! POST /v1/fleets/{id}/integration-requests → handleRequestGrant
//! Workspace-auth operations (list/revoke) are in integration_grants_workspace.zig.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const grant_notifier = @import("../../../fleet_runtime/notifications/grant_notifier.zig");
const api_key = @import("../../../auth/api_key.zig");

const log = logging.scoped(.integration_grants);

pub const Context = common.Context;

/// core.integration_grants.status values. Owned here because handler.zig
/// is the only writer that creates rows; the webhook approval flow imports
/// these to drive its own UPDATE precondition.
const S_PENDING = "pending";
const S_SESSION = "Session ";
const S_REVOKED = "revoked";

pub const GrantStatus = enum {
    const Self = @This();

    pending,
    approved,
    revoked,

    pub fn toSlice(self: Self) []const u8 {
        return switch (self) {
            .pending => S_PENDING,
            .approved => "approved",
            .revoked => S_REVOKED,
        };
    }
};

const STATUS_PENDING = GrantStatus.pending.toSlice();

// ── Fleet auth helpers (Path A + Path B) ─────────────────────────────────
// Mirrors execute.zig auth — caller must prove fleet identity.

const Caller = struct {
    fleet_id: []const u8,
    workspace_id: []const u8,
};

fn fleetFromSession(alloc: std.mem.Allocator, conn: *pg.Conn, token: []const u8) ?Caller {
    var q = PgQuery.from(conn.query(
        \\SELECT s.fleet_id::text, z.workspace_id::text
        \\FROM core.fleet_sessions s
        \\JOIN core.fleets z ON z.id = s.fleet_id
        \\WHERE s.id = $1::uuid
        \\LIMIT 1
    , .{token}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const fleet_id = row.get([]u8, 0) catch return null;
    const workspace_id = row.get([]u8, 1) catch return null;
    return .{
        .fleet_id = alloc.dupe(u8, fleet_id) catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

fn fleetFromApiKey(alloc: std.mem.Allocator, conn: *pg.Conn, raw_key: []const u8) ?Caller {
    const hex = api_key.sha256Hex(raw_key);
    const computed_hash: []const u8 = hex[0..];
    var q = PgQuery.from(conn.query(
        \\SELECT ea.key_hash, ea.fleet_id::text, ea.workspace_id::text
        \\FROM core.fleet_keys ea
        \\WHERE ea.key_hash = $1
        \\LIMIT 1
    , .{computed_hash}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const stored_hash = row.get([]u8, 0) catch return null;
    const fleet_id = row.get([]u8, 1) catch return null;
    const workspace_id = row.get([]u8, 2) catch return null;
    if (!api_key.constantTimeEql(computed_hash, stored_hash)) return null;

    // Best-effort: record last use time. Failure is not fatal.
    _ = conn.exec(
        \\UPDATE core.fleet_keys SET last_used_at = $1 WHERE key_hash = $2
    , .{ clock.nowMillis(), computed_hash }) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });

    return .{
        .fleet_id = alloc.dupe(u8, fleet_id) catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

fn authenticateFleet(alloc: std.mem.Allocator, conn: *pg.Conn, req: *httpz.Request) ?Caller {
    const auth_header = req.header("authorization") orelse return null;
    if (std.mem.startsWith(u8, auth_header, S_SESSION))
        return fleetFromSession(alloc, conn, auth_header[S_SESSION.len..]);
    const bearer_prefix = "Bearer ";
    if (std.mem.startsWith(u8, auth_header, bearer_prefix)) {
        const token = auth_header[bearer_prefix.len..];
        if (std.mem.startsWith(u8, token, api_key.KEY_PREFIX))
            return fleetFromApiKey(alloc, conn, token);
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
// POST /v1/fleets/{fleet_id}/integration-requests
// Fleet authenticates; fleet_id in path must match caller identity.
// Idempotent: returns existing pending grant if one already exists for service.

const RequestGrantBody = struct {
    service: []const u8,
    reason: []const u8,
};

const ExistingGrant = struct {
    fleet_name: []const u8,
    grant_id: []const u8,
    status: []const u8,
    requested_at: i64,
};

pub fn innerRequestGrant(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const caller = authenticateFleet(hx.alloc, conn, req) orelse {
        hx.fail(ec.ERR_APIKEY_INVALID, "Invalid API key or session. Use: Authorization: Session {uuid} or Authorization: Bearer agt_axxx");
        return;
    };

    if (!std.mem.eql(u8, caller.fleet_id, fleet_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Fleet identity mismatch: authenticated fleet_id does not match path");
        return;
    }

    // Defence in depth — verify fleet's workspace matches the path workspace_id.
    if (!std.mem.eql(u8, caller.workspace_id, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace identity mismatch: fleet does not belong to path workspace_id");
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
    //   revoked           → UPDATE back to pending with new reason (re-request allowed)
    //   no row             → INSERT new grant
    const existing = fetchExistingGrant(hx, conn, fleet_id, body.service) orelse {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (existing.grant_id.len != 0) {
        const existing_id = existing.grant_id;
        const existing_st = existing.status;
        const existing_at = existing.requested_at;

        const is_terminal = std.mem.eql(u8, existing_st, S_REVOKED);
        if (!is_terminal) {
            // pending or approved — idempotent return.
            log.debug("already_exists", .{ .fleet_id = fleet_id, .service = body.service, .status = existing_st });
            hx.ok(.ok, .{
                .grant_id = hx.alloc.dupe(u8, existing_id) catch existing_id,
                .fleet_id = fleet_id,
                .service = body.service,
                .status = hx.alloc.dupe(u8, existing_st) catch existing_st,
                .requested_at = existing_at,
                .message = "Grant already exists for this service.",
            });
            return;
        }

        // Revoked: re-request — UPDATE back to pending.
        const now_ms_reopen = clock.nowMillis();
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = $1, requested_at = $2, requested_reason = $3,
            \\    approved_at = NULL, revoked_at = NULL
            \\WHERE grant_id = $4
        , .{ STATUS_PENDING, now_ms_reopen, body.reason, hx.alloc.dupe(u8, existing_id) catch existing_id }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
        log.debug("re_requested", .{ .fleet_id = fleet_id, .service = body.service, .grant_id = existing_id });

        // Notify for the re-request using the existing grant_id.
        const existing_grant_id = hx.alloc.dupe(u8, existing_id) catch existing_id;
        grant_notifier.notifyGrantRequest(
            hx.ctx.pool,
            hx.ctx.queue,
            hx.alloc,
            fleet_id,
            caller.workspace_id,
            existing_grant_id,
            existing.fleet_name,
            body.service,
            body.reason,
        );
        hx.ok(.created, .{
            .grant_id = existing_grant_id,
            .fleet_id = fleet_id,
            .service = body.service,
            .status = STATUS_PENDING,
            .requested_at = now_ms_reopen,
            .message = "Grant re-requested. Awaiting workspace owner approval.",
        });
        return;
    }

    const grant_id = id_format.generateFleetId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();

    _ = conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason)
        \\VALUES ($1::uuid, $1, $2::uuid, $3, $4, $5, $6)
    , .{ grant_id, fleet_id, body.service, STATUS_PENDING, now_ms, body.reason }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.debug("requested", .{ .fleet_id = fleet_id, .service = body.service, .grant_id = grant_id });

    grant_notifier.notifyGrantRequest(
        hx.ctx.pool,
        hx.ctx.queue,
        hx.alloc,
        fleet_id,
        caller.workspace_id,
        grant_id,
        existing.fleet_name,
        body.service,
        body.reason,
    );

    hx.ok(.created, .{
        .grant_id = grant_id,
        .fleet_id = fleet_id,
        .service = body.service,
        .status = S_PENDING,
        .requested_at = now_ms,
        .message = "Grant request submitted. Awaiting workspace owner approval via Slack, Discord, or dashboard.",
    });
}

fn fetchExistingGrant(hx: hx_mod.Hx, conn: *pg.Conn, fleet_id: []const u8, service: []const u8) ?ExistingGrant {
    var existing_q = PgQuery.from(conn.query(
        \\SELECT z.name, COALESCE(g.grant_id, ''), COALESCE(g.status, ''), COALESCE(g.requested_at, 0)
        \\FROM core.fleets z
        \\LEFT JOIN core.integration_grants g
        \\  ON g.fleet_id = z.id
        \\ AND g.service = $2
        \\WHERE z.id = $1::uuid
        \\LIMIT 1
    , .{ fleet_id, service }) catch return null);
    defer existing_q.deinit();

    const existing_row = (existing_q.next() catch return null) orelse return .{
        .fleet_name = fleet_id,
        .grant_id = "",
        .status = "",
        .requested_at = 0,
    };

    return .{
        .fleet_name = hx.alloc.dupe(u8, existing_row.get([]u8, 0) catch fleet_id) catch fleet_id,
        .grant_id = hx.alloc.dupe(u8, existing_row.get([]u8, 1) catch "") catch "",
        .status = hx.alloc.dupe(u8, existing_row.get([]u8, 2) catch "") catch "",
        .requested_at = existing_row.get(i64, 3) catch 0,
    };
}
