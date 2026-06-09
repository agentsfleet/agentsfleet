//! POST /v1/runners/me/heartbeats — runner liveness.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). The S0 request
//! body is empty; the reply is always `{ status: ok }` — `drain`/`stop` arrive
//! with the fleet-failover slice. Side effect: bump `fleet.runners.last_seen_at`
//! (liveness is written here, not on every authed call, per docs/AUTH.md).

const constants = @import("common");
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
        log.warn("heartbeat_acquire_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
        return;
    };
    defer hx.ctx.pool.release(conn);
    const now_ms = clock.nowMillis();
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch |err| {
        log.warn("heartbeat_online_event_id_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
        bumpOnly(conn, runner_id, now_ms);
        return;
    };
    defer hx.alloc.free(event_row_id);
    _ = conn.exec(
        \\WITH locked AS (
        \\  SELECT id, last_seen_at FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
        \\), bumped AS (
        \\  UPDATE fleet.runners r
        \\  SET last_seen_at = $2, updated_at = $2
        \\  FROM locked
        \\  WHERE r.id = locked.id
        \\  RETURNING locked.last_seen_at
        \\)
        \\INSERT INTO fleet.runner_events
        \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
        \\SELECT $3::uuid, $1::uuid, $4::text, $2::bigint,
        \\       jsonb_build_object($5::text, last_seen_at), NULL, $2::bigint
        \\FROM bumped
        \\WHERE last_seen_at = $6 OR ($2::bigint - last_seen_at) > $7
    , .{
        runner_id,
        now_ms,
        event_row_id,
        @tagName(protocol.RunnerEventType.runner_online),
        runner_events.META_LAST_SEEN_AT,
        protocol.RUNNER_LAST_SEEN_NEVER,
        constants.RUNNER_OFFLINE_AFTER_MS,
    }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .runner_id = runner_id, .err = @errorName(err) });
    };
}

fn bumpOnly(conn: anytype, runner_id: []const u8, now_ms: i64) void {
    _ = conn.exec(
        \\UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid
    , .{ runner_id, now_ms }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .runner_id = runner_id, .err = @errorName(err) });
    };
}
