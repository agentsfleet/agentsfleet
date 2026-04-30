//! PATCH /v1/workspaces/{ws}/zombies/{id} — partial update of a zombie.
//!
//! Body fields are optional and presence-based:
//!   - `config_json`?: string  — replace zombie config (XADD zombie_config_changed).
//!   - `status`?: "killed"     — kill the zombie (XADD zombie_status_changed).
//!
//! Both fields can ride together; the SQL UPDATE is one statement and the
//! control-stream publishes (one per dirty surface) follow. Empty body is a
//! 200 no-op — matches REST PATCH semantics. The previous standalone
//! `POST /v1/.../zombies/{id}/kill` endpoint folded into this handler;
//! aborting the in-flight stage (a separate semantic) still lives at
//! `DELETE /v1/.../zombies/{id}/current-run`.
//!
//! `updated_at` (BIGINT, ms epoch) doubles as the config_revision — strictly
//! monotonic per zombie, no schema migration needed.

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

const PatchBody = struct {
    config_json: ?[]const u8 = null,
    status: ?[]const u8 = null,
};

/// Partial update of a zombie. Validates inputs (config_json via
/// parseZombieConfig; status must equal "killed"), persists in one SQL
/// UPDATE, and publishes one control-stream signal per dirty surface.
/// Returns 404 if no row matches (zombie missing, cross-workspace, or
/// already killed — re-killing is idempotent-fail).
pub fn innerPatchZombie(hx: Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const body = parsePatchBody(hx, req) orelse return;
    if (body.config_json == null and body.status == null) {
        // Empty PATCH body — nothing to do. Treat as a 200 no-op.
        // `config_revision: null` (vs `0`) lets callers distinguish "no-op,
        // revision unchanged" from a genuine revision integer.
        hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = @as(?i64, null) });
        return;
    }

    if (body.config_json) |cj| {
        if (cj.len == 0) {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED);
            return;
        }
        _ = zombie_config.parseZombieConfig(hx.alloc, cj) catch {
            hx.fail(ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG);
            return;
        };
    }
    if (body.status) |s| {
        if (!std.mem.eql(u8, s, zombie_config.ZombieStatus.killed.toSlice())) {
            hx.fail(ec.ERR_INVALID_REQUEST, "status must be \"killed\"");
            return;
        }
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

    const revision = patchZombieOnConn(conn, workspace_id, zombie_id, body) catch |err| {
        log.err("zombie.patch_db_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (revision == null) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    if (body.status != null) {
        publishKillSignal(hx.ctx.queue, zombie_id) catch |err| {
            // PG row is `status='killed'` already. Watcher's
            // `reconcileCancelNonActive` (≈30s cadence) heals this case.
            log.warn(
                "zombie.kill_publish_failed err={s} zombie_id={s} req_id={s} hint=row_updated_reconcile_will_cancel_within_30s",
                .{ @errorName(err), zombie_id, hx.req_id },
            );
        };
    }
    if (body.config_json != null) {
        publishConfigChange(hx.ctx.queue, zombie_id, revision.?) catch |err| {
            log.warn(
                "zombie.patch_publish_failed err={s} zombie_id={s} revision={d} req_id={s} hint=row_updated_worker_blind",
                .{ @errorName(err), zombie_id, revision.?, hx.req_id },
            );
        };
    }

    log.info("zombie.patched id={s} workspace={s} revision={d} status_set={?s}", .{ zombie_id, workspace_id, revision.?, body.status });
    if (body.status) |s| {
        hx.ok(.ok, .{
            .zombie_id = zombie_id,
            .status = s,
            .config_revision = revision.?,
        });
    } else {
        hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = revision.? });
    }
}

fn parsePatchBody(hx: Hx, req: *httpz.Request) ?PatchBody {
    const body = req.body() orelse return PatchBody{};
    if (body.len == 0) return PatchBody{};
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn patchZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, body: PatchBody) !?i64 {
    const now_ms = std.time.milliTimestamp();
    // One UPDATE handles either or both fields; COALESCE preserves the
    // existing column when the corresponding body field is null. Gate on
    // `status != killed` so re-killing a killed zombie returns null and the
    // caller surfaces 404 (idempotent-fail). The ZombieStatus enum is the
    // single source of truth for the literal — hardcoding 'killed' in SQL
    // would drift on rename.
    const killed = zombie_config.ZombieStatus.killed.toSlice();
    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET
        \\    config_json = COALESCE($1::jsonb, config_json),
        \\    status = COALESCE($2, status),
        \\    updated_at = $3
        \\WHERE id = $4::uuid AND workspace_id = $5::uuid AND status != $6
        \\RETURNING updated_at
    , .{ body.config_json, body.status, now_ms, zombie_id, workspace_id, killed }));
    defer q.deinit();
    if (try q.next()) |row| return try row.get(i64, 0);
    return null;
}

fn publishConfigChange(redis: *queue_redis.Client, zombie_id: []const u8, revision: i64) !void {
    try control_stream.publish(redis, .{
        .zombie_config_changed = .{
            .zombie_id = zombie_id,
            .config_revision = revision,
        },
    });
}

fn publishKillSignal(redis: *queue_redis.Client, zombie_id: []const u8) !void {
    try control_stream.publish(redis, .{
        .zombie_status_changed = .{
            .zombie_id = zombie_id,
            .status = .killed,
        },
    });
}
