//! POST /v1/runners/me/heartbeats — runner liveness.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). The S0 request
//! body is empty; the reply is always `{ status: ok }` — `drain`/`stop` arrive
//! with the fleet-failover slice. Side effect: bump `fleet.runners.last_seen_at`
//! (liveness is written here, not on every authed call, per docs/AUTH.md).

const constants = @import("common");
const sql = @import("sql.zig");
const clock = constants.clock;
const logging = @import("log");
const httpz = @import("httpz");

const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const metrics_runner = @import("../../../observability/metrics_runner.zig");
const id_format = @import("../../../types/id_format.zig");
const runner_events = @import("../../../fleet/runner_events.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_heartbeat);
const LOG_EVENT_HEARTBEAT_BUMP_FAILED = "heartbeat_bump_failed";

pub fn innerRunnerHeartbeat(hx: Hx, req: *httpz.Request) void {
    _ = req; // S0 request body is empty.
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this is set; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    bumpLastSeen(hx, runner_id);
    metrics_runner.touchRunnerSeen(runner_id); // in-memory liveness for /metrics
    hx.ok(.ok, protocol.HeartbeatResponse{ .status = .ok });
}

/// Best-effort liveness bump — a DB blip must not fail the heartbeat reply.
fn bumpLastSeen(hx: Hx, runner_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch |err| {
        log.warn("heartbeat_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .runner_id = runner_id, .err = @errorName(err) });
        return;
    };
    defer hx.ctx.pool.release(conn);
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
