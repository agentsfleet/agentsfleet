//! POST /v1/runners/me/heartbeats — runner liveness.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). The S0 request
//! body is empty; the reply is always `{ status: ok }` — `drain`/`stop` arrive
//! with the fleet-failover slice. Side effect: bump `fleet.runners.last_seen_at`
//! (liveness is written here, not on every authed call, per docs/AUTH.md).

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");

const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_heartbeat);

pub fn innerRunnerHeartbeat(hx: Hx, req: *httpz.Request) void {
    _ = req; // S0 request body is empty.
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this is set; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    bumpLastSeen(hx, runner_id);
    hx.ok(.ok, protocol.HeartbeatResponse{ .status = .ok });
}

/// Best-effort liveness bump — a DB blip must not fail the heartbeat reply.
fn bumpLastSeen(hx: Hx, runner_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch |err| {
        log.warn("heartbeat_acquire_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
        return;
    };
    defer hx.ctx.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid
    , .{ runner_id, now_ms }) catch |err| {
        log.warn("heartbeat_bump_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
    };
}
