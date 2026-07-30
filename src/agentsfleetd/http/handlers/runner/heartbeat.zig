//! POST /v1/runners/me/heartbeats — runner liveness + policy delivery.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). Every reply
//! carries the row's current assigned policy and degraded verdict, so an
//! operator's dashboard change reaches the host within one heartbeat and a
//! host visit is never needed. `drain`/`stop` statuses arrive with the
//! fleet-failover slice. Side effect: bump `fleet.runners.last_seen_at`
//! (liveness is written here, not on every authed call, per docs/AUTH.md).

const std = @import("std");
const constants = @import("common");
const sql = @import("sql.zig");
const clock = constants.clock;
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const metrics_runner = @import("../../../observability/metrics_runner.zig");
const id_format = @import("../../../types/id_format.zig");
const runner_events = @import("../../../fleet/runner_events.zig");
const policy_row = @import("assigned_policy_row.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_heartbeat);
const LOG_EVENT_HEARTBEAT_BUMP_FAILED = "heartbeat_bump_failed";

pub fn innerRunnerHeartbeat(hx: Hx, req: *httpz.Request) void {
    _ = req; // the capability_report body is consumed by the reconciliation slice
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this is set; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // The policy read is load-bearing (the reply is the delivery channel), so
    // it fails loud — the runner retries and keeps its last-applied policy for
    // in-flight work. The liveness bump below stays best-effort.
    const reply = readPolicyReply(hx.alloc, conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        // Token authenticated but the row is gone (revoked + reaped) — fail
        // closed rather than 200 a phantom runner.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner not found");
        return;
    };
    bumpLastSeen(hx, conn, runner_id);
    metrics_runner.touchRunnerSeen(runner_id); // in-memory liveness for /metrics
    hx.ok(.ok, reply);
}

/// The row's assignment + reconciled verdict, decoded through the shared
/// decoder. The degraded reason is duped into the request arena because the
/// row buffer dies with the local query result.
fn readPolicyReply(alloc: std.mem.Allocator, conn: *pg.Conn, runner_id: []const u8) !?protocol.HeartbeatResponse {
    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_ASSIGNED_POLICY, .{runner_id}) catch return error.DbError);
    defer q.deinit();
    const row = (q.next() catch return error.DbError) orelse return null;
    const assigned = policy_row.decodePolicy(
        alloc,
        row.get([]const u8, 0) catch return error.DbError,
        row.get(?[]const u8, 1) catch return error.DbError,
        row.get(?[]const u8, 2) catch return error.DbError,
        row.get(i32, 3) catch return error.DbError,
    );
    const reason_raw = row.get(?[]const u8, 5) catch return error.DbError;
    const reason = if (reason_raw) |r| try alloc.dupe(u8, r) else null;
    return protocol.HeartbeatResponse{
        .status = .ok,
        .assigned_policy = assigned,
        .degraded = row.get(bool, 4) catch return error.DbError,
        .degraded_reason = reason,
    };
}

/// Best-effort liveness bump — a DB blip must not fail the heartbeat reply.
fn bumpLastSeen(hx: Hx, conn: *pg.Conn, runner_id: []const u8) void {
    const now_ms = clock.nowMillis();
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch |err| {
        log.warn("heartbeat_online_event_id_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
        bumpOnly(conn, runner_id, now_ms);
        return;
    };
    defer hx.alloc.free(event_row_id);
    _ = conn.exec(sql.HEARTBEAT_WITH_TRANSITION_EVENT, .{
        runner_id,
        now_ms,
        event_row_id,
        @tagName(protocol.RunnerEventType.runner_online),
        runner_events.META_LAST_SEEN_AT,
        protocol.RUNNER_LAST_SEEN_NEVER,
        constants.RUNNER_OFFLINE_AFTER_MS,
    }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .runner_id = runner_id, .err = @errorName(err) });
        bumpOnly(conn, runner_id, now_ms);
    };
}

fn bumpOnly(conn: anytype, runner_id: []const u8, now_ms: i64) void {
    _ = conn.exec(sql.TOUCH_RUNNER_LAST_SEEN, .{ runner_id, now_ms }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
    };
}
