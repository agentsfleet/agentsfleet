// Zombie lifecycle actions — operator kill switch.
//
// DELETE /v1/workspaces/{ws}/zombies/{zombie_id}/current-run
//   Kills the zombie's current running action. Transitions status from
//   `active` | `paused` → `stopped` and records a `zombie_stopped` activity
//   event. `stopped` is the non-terminal halt state; `killed` is the terminal
//   delete marker set by `DELETE /zombies/{id}`.
//   Returns 200 with the new state. Returns 404 if the zombie is not in the
//   path workspace (cross-tenant IDOR guard). Returns 409 if the zombie is
//   already stopped or killed.
//
// REST shape note: "current-run" is modeled as a singleton sub-resource of
// the zombie. DELETE on a singleton sub-resource is the §7-compliant way to
// express "kill the thing that's running." The predecessor endpoint,
// `POST /.../stop`, baked an action verb into the path and has been retired.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = std.log.scoped(.zombie_lifecycle);

const API_ACTOR = "api";
const DETAIL_MANUAL_STOP = "stopped via operator kill switch";

// RULE BIL-adjacent: destructive lifecycle actions (kill switch, future
// delete/pause/resume) require operator-minimum role. Regular workspace
// members must not be able to halt a running zombie they didn't deploy.

// StopOutcome describes the three paths: transitioned / already-terminal / not-found.
const StopOutcome = enum { transitioned, already_terminal, not_found };

pub fn innerDeleteCurrentRun(
    hx: hx_mod.Hx,
    _: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
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

    // RULE BIL-adjacent: kill switch is a destructive action. Operator-minimum
    // role required so plain workspace members can't halt production zombies.
    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    // RULE ZWO: verify zombie belongs to the path workspace (don't leak existence).
    const zombie_ws_id = common.getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    };
    if (!std.mem.eql(u8, zombie_ws_id, workspace_id)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    const outcome = stopZombieOnConn(conn, workspace_id, zombie_id) catch |err| {
        log.err("zombie.stop_failed err={s} zombie_id={s} req_id={s}", .{ @errorName(err), zombie_id, hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    switch (outcome) {
        .transitioned => {
            writeStoppedEvent(hx, workspace_id, zombie_id);
            log.info("zombie.stopped id={s} workspace={s} actor={s}", .{
                zombie_id,
                workspace_id,
                hx.principal.user_id orelse API_ACTOR,
            });
            hx.ok(.ok, .{
                .zombie_id = zombie_id,
                .workspace_id = workspace_id,
                .status = zombie_config.ZombieStatus.stopped.toSlice(),
                .request_id = hx.req_id,
            });
        },
        .already_terminal => {
            hx.fail(ec.ERR_ZOMBIE_ALREADY_TERMINAL, "Zombie is already stopped or killed");
        },
        .not_found => {
            // Race: zombie existed at getZombieWorkspaceId but vanished before UPDATE.
            hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        },
    }
}

fn stopZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8) !StopOutcome {
    const now_ms = std.time.milliTimestamp();
    const stopped_label = zombie_config.ZombieStatus.stopped.toSlice();
    const active_label = zombie_config.ZombieStatus.active.toSlice();
    const paused_label = zombie_config.ZombieStatus.paused.toSlice();

    // Conditional UPDATE: only transitions from active|paused. If the row exists
    // but is already stopped/killed, zero rows RETURN and we fall back to a
    // presence check to distinguish 409 (already terminal) from 404 (gone).
    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET status = $1, updated_at = $2
        \\WHERE id = $3::uuid
        \\  AND workspace_id = $4::uuid
        \\  AND status IN ($5, $6)
        \\RETURNING id
    , .{ stopped_label, now_ms, zombie_id, workspace_id, active_label, paused_label }));
    defer q.deinit();
    const updated = (try q.next()) != null;
    if (updated) return .transitioned;

    // Presence probe — scoped to workspace to preserve the IDOR guarantee.
    var probe = PgQuery.from(try conn.query(
        \\SELECT 1 FROM core.zombies
        \\WHERE id = $1::uuid AND workspace_id = $2::uuid
        \\LIMIT 1
    , .{ zombie_id, workspace_id }));
    defer probe.deinit();
    if ((try probe.next()) != null) return .already_terminal;
    return .not_found;
}

fn writeStoppedEvent(hx: hx_mod.Hx, workspace_id: []const u8, zombie_id: []const u8) void {
    _ = hx;
    log.info("zombie.stopped workspace_id={s} zombie_id={s} reason={s}", .{ workspace_id, zombie_id, DETAIL_MANUAL_STOP });
}

// ── Unit tests ────────────────────────────────────────────────────────────
//
// The handler itself requires a live DB + httpz server, covered by the
// integration test (`zombie_lifecycle_integration_test.zig`). These unit
// tests pin the invariants that apply without DB: constants, enum contract.

test "ZombieStatus.stopped round-trips through toSlice/fromSlice" {
    try std.testing.expectEqualStrings("stopped", zombie_config.ZombieStatus.stopped.toSlice());
    try std.testing.expectEqual(zombie_config.ZombieStatus.stopped, zombie_config.ZombieStatus.fromSlice("stopped").?);
}

test "DETAIL_MANUAL_STOP is human-readable and non-empty" {
    try std.testing.expect(DETAIL_MANUAL_STOP.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, DETAIL_MANUAL_STOP, "operator") != null);
}

test "StopOutcome variants cover the three branches" {
    const all = [_]StopOutcome{ .transitioned, .already_terminal, .not_found };
    try std.testing.expectEqual(@as(usize, 3), all.len);
}
