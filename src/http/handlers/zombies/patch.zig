//! PATCH /v1/workspaces/{ws}/zombies/{id} — partial update of zombie config.
//!
//! Body: `{ "config_json"?: string }`. UPDATE core.zombies SET config_json,
//! updated_at + XADD zombie:control zombie_config_changed config_revision=
//! updated_at. The watcher reads the control message and (per the M40 spec)
//! logs the change for now; full hot-reload semantics are wired in M41.
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
};

/// Partial update of zombie config_json. Validates the new config via
/// zombie_config.parseZombieConfig before persisting. Returns the new
/// config_revision (the `updated_at` timestamp, strictly monotonic per
/// zombie). Empty body is a 200 no-op — matches REST PATCH semantics.
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
    const new_config = body.config_json orelse {
        // Empty PATCH body — nothing to do. Treat as a 200 no-op.
        hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = @as(i64, 0) });
        return;
    };

    if (new_config.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED);
        return;
    }
    _ = zombie_config.parseZombieConfig(hx.alloc, new_config) catch {
        hx.fail(ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const revision = patchZombieOnConn(conn, workspace_id, zombie_id, new_config) catch |err| {
        log.err("zombie.patch_db_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (revision == null) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    publishConfigChange(hx.ctx.queue, zombie_id, revision.?) catch |err| {
        log.warn(
            "zombie.patch_publish_failed err={s} zombie_id={s} revision={d} req_id={s} hint=row_updated_worker_blind",
            .{ @errorName(err), zombie_id, revision.?, hx.req_id },
        );
    };

    log.info("zombie.patched id={s} workspace={s} revision={d}", .{ zombie_id, workspace_id, revision.? });
    hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = revision.? });
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

fn patchZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, config_json: []const u8) !?i64 {
    const now_ms = std.time.milliTimestamp();
    // Gate on `status != killed` (greptile P2 on PR #251) so a PATCH against
    // a killed zombie returns null → caller surfaces 404. Mirrors `kill.zig`'s
    // `status != $1` pattern with `ZombieStatus.killed.toSlice()` — keeps the
    // enum the single source of truth for status literals (CLAUDE.md
    // "Engineering Discipline → Code Structure"). Hardcoded `'killed'` in the
    // SQL would drift from the Zig enum on rename.
    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET config_json = $1::jsonb, updated_at = $2
        \\WHERE id = $3::uuid AND workspace_id = $4::uuid AND status != $5
        \\RETURNING updated_at
    , .{ config_json, now_ms, zombie_id, workspace_id, zombie_config.ZombieStatus.killed.toSlice() }));
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
