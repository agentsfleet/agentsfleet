// M23_001 / M24_001: POST /v1/workspaces/{ws}/zombies/{id}/steer — live steering for active zombies.
//
// Verifies zombie ownership, writes message to Redis key zombie:{id}:steer (SETEX
// 300s TTL). The worker polls this key at the top of each event loop iteration and
// injects the message as a synthetic "steer" event into the zombie's event stream.
//
// Response includes execution_id (from core.zombie_sessions) so the caller can tell
// whether the message lands mid-execution or queued for the zombie's next event.
//
// Auth: Bearer token with workspace scope (registry.bearer()).
// Scope check: zombie must belong to the caller's workspace.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (core.zombies, core.zombie_sessions).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const queue_consts = @import("../../../queue/constants.zig");

const log = std.log.scoped(.zombie_steer);

const Hx = hx_mod.Hx;

const MAX_MESSAGE_LEN: usize = 8192;

const SteerBody = struct {
    message: []const u8,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn innerZombieSteer(hx: Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    // Parse + validate body.
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(
        SteerBody,
        hx.alloc,
        body_raw,
        .{ .ignore_unknown_fields = true },
    ) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    const msg = parsed.value.message;
    if (msg.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, "message must not be empty");
        return;
    }
    if (msg.len > MAX_MESSAGE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "message must not exceed 8192 bytes");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // M24_001 / RULE WAUTH: principal must have access to the path workspace_id.
    //
    // Intentional widening vs M23_001: the original spec required
    // `principal.workspace_scope_id != null` (operator-token-only). M24_001 drops
    // that pre-check because `authorizeWorkspace` is the canonical workspace authZ
    // gate used by every workspace-scoped handler — special-casing /steer to also
    // require a workspace-scoped token would be inconsistent with create/list/
    // delete/activity/credentials/grants. Membership-based principals with access
    // to the workspace may now steer. If a role gate is ever needed (e.g. only
    // `.operator`), add `common.requireRole(…, .operator)` here — don't re-add
    // the token-scope check.
    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    // Verify zombie exists + belongs to path workspace; read active execution_id if any.
    const exec_id_opt = resolveZombieExecution(hx, conn, workspace_id, zombie_id) orelse return;

    // Write steer signal to Redis. Worker polls GETDEL at top of event loop.
    const steer_key = std.fmt.allocPrint(
        hx.alloc,
        "zombie:{s}{s}",
        .{ zombie_id, queue_consts.zombie_steer_key_suffix },
    ) catch {
        common.internalOperationError(hx.res, "OOM building steer key", hx.req_id);
        return;
    };

    hx.ctx.queue.setEx(steer_key, msg, queue_consts.zombie_steer_ttl_seconds) catch |err| {
        log.warn("zombie_steer.redis_write_failed zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        hx.ok(.ok, .{
            .message_queued = false,
            .execution_active = exec_id_opt != null,
            .execution_id = @as(?[]const u8, null),
        });
        return;
    };

    const is_active = exec_id_opt != null;
    log.info("zombie_steer.steered zombie_id={s} execution_active={}", .{ zombie_id, is_active });
    hx.ok(.ok, .{
        .message_queued = true,
        .execution_active = is_active,
        .execution_id = exec_id_opt,
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Verify zombie belongs to caller's workspace. Returns active execution_id (null
/// if zombie is idle). Returns null and writes error response on ownership failure.
///
/// ??[]const u8 semantics:
///   outer null = error, response already written — caller must return
///   inner null = zombie exists, owned, but not currently executing (idle state)
fn resolveZombieExecution(
    hx: Hx,
    conn: *pg.Conn,
    path_workspace_id: []const u8,
    zombie_id: []const u8,
) ??[]const u8 {
    // M24_001: verify zombie belongs to the path workspace_id.
    // Returns 404 (not 403) on mismatch — do not leak existence across workspaces.
    var q1 = PgQuery.from(conn.query(
        "SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid",
        .{zombie_id},
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q1.deinit();

    const row1 = (q1.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return null;
    };

    const zombie_workspace = row1.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };

    if (!std.mem.eql(u8, path_workspace_id, zombie_workspace)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return null;
    }

    // Step 2: read active execution_id (M23_001 column — NULL when idle).
    // PgQuery.deinit() on q1 drains it before we open q2 (RULE FLS).
    var q2 = PgQuery.from(conn.query(
        \\SELECT execution_id
        \\FROM core.zombie_sessions
        \\WHERE zombie_id = $1::uuid
        \\  AND execution_id IS NOT NULL
        \\LIMIT 1
    , .{zombie_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q2.deinit();

    const row2 = (q2.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse return @as(?[]const u8, null); // idle — valid state

    const raw = row2.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };

    // Dupe before q2 deinit fires (RULE FLS — borrowed row data).
    return hx.alloc.dupe(u8, raw) catch {
        common.internalOperationError(hx.res, "OOM duping execution_id", hx.req_id);
        return null;
    };
}
