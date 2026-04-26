//! DELETE /v1/workspaces/{ws}/zombies/{id} — legacy "kill via DELETE" path.
//!
//! Slated for removal in §6 of the worker substrate spec — the new
//! POST /kill endpoint replaces it with a clean verb + control-stream
//! publish. This file exists only briefly during the §5 split so the
//! pre-existing endpoint keeps working until §6 lands the kill handler
//! and removes both the route and this file.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");

const log = std.log.scoped(.zombie_api);

const Hx = hx_mod.Hx;

pub fn innerDeleteZombie(hx: Hx, _: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
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
        log.err("zombie.delete_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (!updated) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    log.info("zombie.killed id={s} workspace={s}", .{ zombie_id, workspace_id });
    hx.ok(.ok, .{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.killed.toSlice() });
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
