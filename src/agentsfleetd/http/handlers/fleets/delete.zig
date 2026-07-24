//! DELETE /v1/workspaces/{ws}/fleets/{id} — hard-delete a fleet.
//!
//! Precondition: status='killed' (kill-before-purge). The two-step lifecycle
//! protects against accidental data loss — operator must mark terminal first,
//! then explicitly purge. Returns 409 if the fleet isn't killed yet.
//!
//! Cascade:
//!   - core.fleet_events           — FK ON DELETE CASCADE (auto)
//!   - core.integration_grants      — FK ON DELETE CASCADE (auto)
//!   - core.fleet_keys              — FK ON DELETE CASCADE (auto)
//!   - core.fleet_sessions         — no FK cascade; explicit DELETE
//!   - core.fleet_approval_gates   — no FK cascade; explicit DELETE
//!   - memory.memory_entries        — keyed by fleet_id (UUID, no FK); explicit
//!   - fleet_execution_telemetry   — keyed by fleet_id (no FK); explicit
//!   - fleet:{id}:events Redis stream — best-effort DEL after PG commit
//!
//! Auth: operator-minimum role per RULE BIL — destructive lifecycle action.

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const approval_gate_db = @import("../../../fleet_runtime/approval_gate_db.zig");
const common = @import("../common.zig");
const CronStore = @import("../../../cron/Store.zig");
const cron_sync = @import("cron_sync.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.fleet_api);
const API_ACTOR = "api";
/// Best-effort rollback/cleanup failure that is logged and swallowed (RULE UFS — 2 sites).
const EVENT_IGNORED_ERROR = "ignored_error";

const Hx = hx_mod.Hx;

const DeleteOutcome = enum { purged, not_killed, not_found };

const S_ROLLBACK = "ROLLBACK";

pub fn innerDeleteFleet(hx: Hx, _: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a valid UUIDv7");
        return;
    }

    const actor = hx.principal.user_id orelse API_ACTOR;
    {
        const conn = hx.ctx.pool.acquire() catch {
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        };
        defer hx.ctx.pool.release(conn);
        const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
        defer access.deinit(hx.alloc);
    }

    const belongs = CronStore.init(hx.ctx.pool).fleetBelongsToWorkspace(fleet_id, workspace_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (!belongs) {
        hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return;
    }

    const cron_result = cron_sync.removeAll(hx, fleet_id);
    if (cron_result != .ok and cron_result != .skipped) {
        _ = cron_sync.writeFailure(hx, cron_result);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const outcome = purgeFleetOnConn(conn, workspace_id, fleet_id) catch |err| {
        log.err("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .fleet_id = fleet_id, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    switch (outcome) {
        .purged => {
            // Best-effort Redis cleanup. PG state is the source of truth; if
            // the stream DEL fails the row is already gone, so the next
            // worker XREADGROUP will see the empty stream and move on.
            cleanupRedisStream(hx.ctx.queue, fleet_id) catch |err| {
                log.warn(
                    "delete_redis_cleanup_failed",
                    .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .fleet_id = fleet_id, .req_id = hx.req_id, .hint = "pg_row_purged_stream_orphaned_until_ttl" },
                );
            };
            log.debug("purged", .{ .id = fleet_id, .workspace = workspace_id, .actor = actor });
            hx.res.status = 204;
        },
        .not_killed => hx.fail(ec.ERR_AGENTSFLEET_ALREADY_TERMINAL, "Fleet must be killed before delete (PATCH status=killed first)"),
        .not_found => hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND),
    }
}

/// Single-transaction purge. The non-cascading child tables are wiped first,
/// then the parent row; PG-level CASCADE handles the rest. The whole sequence
/// runs inside BEGIN/COMMIT so a failure mid-stream rolls back, leaving the
/// caller free to retry without partial-purge wreckage.
fn purgeFleetOnConn(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8) !DeleteOutcome {
    const killed = fleet_config.FleetStatus.killed.toSlice();

    // Pre-flight: classify before any DELETE so we return distinct 404 vs 409
    // without partial mutation.
    {
        var probe = PgQuery.from(try conn.query(sql.SELECT_FLEET_STATUS, .{ fleet_id, workspace_id }));
        defer probe.deinit();
        const row = (try probe.next()) orelse return .not_found;
        const cur_status = try row.get([]const u8, 0);
        if (!std.mem.eql(u8, cur_status, killed)) return .not_killed;
    }

    _ = try conn.exec("BEGIN", .{});
    // Registered BEFORE the bypass SET LOCAL so its failure also rolls back.
    // conn.rollback(), not exec("ROLLBACK") — exec short-circuits on a
    // FAIL-state connection after a statement error, leaving the session
    // stuck in the aborted transaction (signup_bootstrap.zig precedent).
    errdefer conn.rollback() catch |err| log.warn(EVENT_IGNORED_ERROR, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    _ = try conn.exec(approval_gate_db.SET_GATE_PURGE_BYPASS_SQL, .{});

    _ = try conn.exec(
        "DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1 AND fleet_id = $2",
        .{ workspace_id, fleet_id },
    );
    _ = try conn.exec(
        "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid",
        .{fleet_id},
    );
    _ = try conn.exec(
        "DELETE FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
        .{fleet_id},
    );
    _ = try conn.exec(
        "DELETE FROM core.fleet_sessions WHERE fleet_id = $1::uuid",
        .{fleet_id},
    );
    // Final delete; PG CASCADE handles fleet_events / integration_grants /
    // fleet_keys. Status guard is belt-and-suspenders against TOCTOU between
    // the pre-flight probe and here — if a concurrent PATCH resurrected the
    // fleet, the row count is 0 and we surface 409.
    //
    // Scoped so PgQuery.deinit() drains the RETURNING result before the
    // COMMIT/ROLLBACK below: exec() on a connection still holding an in-flight
    // result throws ConnectionBusy, which then poisons the pooled connection
    // (left mid-transaction) for the next acquirer.
    const purged = blk: {
        var del = PgQuery.from(try conn.query(sql.DELETE_FLEET_IN_STATUS, .{ fleet_id, workspace_id, killed }));
        defer del.deinit();
        break :blk (try del.next()) != null;
    };
    if (!purged) {
        _ = conn.exec(S_ROLLBACK, .{}) catch |err| log.warn(EVENT_IGNORED_ERROR, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return .not_killed;
    }
    _ = try conn.exec("COMMIT", .{});
    return .purged;
}

fn cleanupRedisStream(redis: *queue_redis.Client, fleet_id: []const u8) !void {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var resp = try redis.commandAllowError(&.{ "DEL", key });
    resp.deinit(redis.alloc);
}
