// M23_001: POST /v1/zombies/{id}:steer — live steering for active runs.
//
// Looks up the zombie's active execution_id from core.zombie_sessions.
// If found, writes the message to Redis key run:{execution_id}:interrupt
// so the worker gate loop picks it up on the next checkpoint.
//
// Auth: Bearer token with workspace scope (registry.bearer()).
// Scope check: zombie must belong to the caller's workspace.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (core.zombies, core.zombie_sessions).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const queue_consts = @import("../../queue/constants.zig");

const log = std.log.scoped(.zombie_steer);

const Hx = hx_mod.Hx;

const MAX_MESSAGE_LEN: usize = 8192;

const SteerBody = struct {
    message: []const u8,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn innerZombieSteer(hx: Hx, req: *httpz.Request, zombie_id: []const u8) void {
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

    // Verify zombie exists and belongs to the caller's workspace.
    const exec_id_opt = resolveActiveExecution(hx, conn, zombie_id) orelse return;

    // If no active run: return ack with run_steered=false.
    const exec_id = exec_id_opt orelse {
        hx.ok(.ok, .{
            .ack = true,
            .run_steered = false,
            .execution_id = @as(?[]const u8, null),
        });
        return;
    };

    // Write interrupt key to Redis. TTL matches M21_001 constant.
    const redis_key = std.fmt.allocPrint(
        hx.alloc,
        "{s}{s}",
        .{ queue_consts.interrupt_key_prefix, exec_id },
    ) catch {
        common.internalOperationError(hx.res, "OOM building interrupt key", hx.req_id);
        return;
    };

    hx.ctx.queue.setEx(redis_key, msg, queue_consts.interrupt_ttl_seconds) catch |err| {
        log.warn("zombie_steer.redis_write_failed execution_id={s} err={s}", .{ exec_id, @errorName(err) });
        // Redis failure is non-fatal: return ack with run_steered=false so
        // the caller knows the message was not delivered.
        hx.ok(.ok, .{
            .ack = true,
            .run_steered = false,
            .execution_id = @as(?[]const u8, null),
        });
        return;
    };

    log.info("zombie_steer.steered zombie_id={s} execution_id={s}", .{ zombie_id, exec_id });
    hx.ok(.ok, .{
        .ack = true,
        .run_steered = true,
        .execution_id = exec_id,
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Verify zombie belongs to the caller's workspace, then return active execution_id
/// (null if no active session). Returns null and writes an error response on failure.
///
/// Two-query path:
///   1. Verify zombie exists + workspace match.
///   2. Look up active execution_id from core.zombie_sessions.
///
/// Cross-workspace access returns 404 — not 403 — to avoid existence leaks.
fn resolveActiveExecution(
    hx: Hx,
    conn: *pg.Conn,
    zombie_id: []const u8,
) ??[]const u8 {
    // Step 1: verify ownership.
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

    const caller_ws = hx.principal.workspace_scope_id orelse {
        hx.fail(ec.ERR_UNAUTHORIZED, "workspace-scoped token required");
        return null;
    };
    if (!std.mem.eql(u8, caller_ws, zombie_workspace)) {
        // Return 404 not 403 — do not leak existence across workspaces.
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return null;
    }

    // Step 2: look up active execution_id.
    // Drain q1 before opening q2 on the same connection (RULE FLS).
    // PgQuery.deinit() already drains, so defer above covers this.

    var q2 = PgQuery.from(conn.query(
        \\SELECT execution_id::text
        \\FROM core.zombie_sessions
        \\WHERE zombie_id = $1::uuid
        \\  AND execution_id IS NOT NULL
        \\ORDER BY created_at DESC
        \\LIMIT 1
    , .{zombie_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q2.deinit();

    const row2 = (q2.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse return @as(?[]const u8, null); // no active session — valid state

    const raw = row2.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };

    // Dupe before q2 deinit fires (RULE FLS — borrowed row data).
    const exec_id = hx.alloc.dupe(u8, raw) catch {
        common.internalOperationError(hx.res, "OOM duping execution_id", hx.req_id);
        return null;
    };
    return exec_id;
}
