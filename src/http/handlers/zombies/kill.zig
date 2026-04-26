//! POST /v1/workspaces/{ws}/zombies/{id}/kill — abort an in-flight zombie.
//!
//! UPDATE core.zombies SET status='killed' + XADD zombie:control
//! zombie_status_changed status=killed. The watcher reads the control
//! message, flips the per-zombie cancel flag, and (if mid-execution) calls
//! the executor's cancelExecution. The per-zombie thread observes the
//! cancel flag at the top of its loop and exits within ~100ms.
//!
//! Replaces the legacy DELETE /v1/.../zombies/{id} verb (per F4 of the
//! M40 grounding pass — see ARCHITECHTURE.md and the spec amendment).
//! Pre-v2.0 era: no 410 stub on the old DELETE path; it 404s cleanly.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const control_stream = @import("../../../zombie/control_stream.zig");

const log = std.log.scoped(.zombie_api);

const Hx = hx_mod.Hx;

/// Marks the zombie as killed in core.zombies and publishes the
/// zombie_status_changed control-stream signal. Returns 404 if the row is
/// missing or already killed (idempotent semantics fold into 404). Best-
/// effort on the publish step — DB row is the source of truth, control
/// publish failure logs at warn but does not roll back.
pub fn innerKillZombie(hx: Hx, _: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const updated = killZombieOnConn(conn, workspace_id, zombie_id) catch |err| {
        log.err("zombie.kill_db_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (!updated) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    publishKillSignal(hx.ctx.queue, zombie_id) catch |err| {
        // Status row updated; control publish failed → worker will pick it up
        // on the next reconcile sweep but the user-visible response shows
        // best-effort completion. Log loud so ops sees the divergence.
        log.warn(
            "zombie.kill_publish_failed err={s} zombie_id={s} req_id={s} hint=row_updated_worker_blind",
            .{ @errorName(err), zombie_id, hx.req_id },
        );
    };

    log.info("zombie.killed id={s} workspace={s}", .{ zombie_id, workspace_id });
    hx.ok(.ok, .{
        .zombie_id = zombie_id,
        .status = zombie_config.ZombieStatus.killed.toSlice(),
        .queued_at = std.time.milliTimestamp(),
    });
}

fn killZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8) !bool {
    const now_ms = std.time.milliTimestamp();
    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET status = $1, updated_at = $2
        \\WHERE id = $3::uuid AND workspace_id = $4::uuid AND status != $1
        \\RETURNING id
    , .{ zombie_config.ZombieStatus.killed.toSlice(), now_ms, zombie_id, workspace_id }));
    defer q.deinit();
    return try q.next() != null;
}

fn publishKillSignal(redis: *queue_redis.Client, zombie_id: []const u8) !void {
    try control_stream.publish(redis, .{
        .zombie_status_changed = .{
            .zombie_id = zombie_id,
            .status = .killed,
        },
    });
}
