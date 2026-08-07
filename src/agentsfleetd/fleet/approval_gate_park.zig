//! The park tail every approval shares: serialize the pending action, stage the
//! card payload, record the durable row and the event→gate ref.
//!
//! Two callers produce a parked event — the rules path (a matched `GateRule`)
//! and the write-kind path (a fleet whose repository binding declares write
//! access). The tail is identical, and drift between two copies
//! would let one park record a ref the other's spelling could not resolve, so
//! it lives once here.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const approval_gate = @import("../fleet_runtime/approval_gate.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const error_codes = @import("../errors/error_registry.zig");
const FleetSession = @import("fleet_session.zig");
const logging = @import("log");

const log = logging.scoped(.fleet_event_loop_gate);

pub const ParkOutcome = enum { parked, unavailable };

/// Best-effort gate-event log. Gate transitions currently emit structured
/// logs; durable terminal state lands in core.fleet_events via the worker's
/// terminal UPDATE.
pub fn logGateActivity(pool: *pg.Pool, alloc: Allocator, session: *FleetSession, event_type: []const u8, detail: []const u8) void {
    _ = pool;
    _ = alloc;
    log.debug("gate_event", .{ .fleet_id = session.fleet_id, .workspace_id = session.workspace_id, .type = event_type, .detail = detail });
}

/// Park `event` behind `detail`'s approval. Any Redis loss returns
/// `.unavailable` — fail closed (default-deny), never a silently released run.
/// `detail.timeout_ms` sets the ref deadline, so the two paths cannot disagree
/// about when an unanswered card expires.
pub fn parkEvent(
    alloc: Allocator,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    detail: approval_gate.ActionDetail,
) ParkOutcome {
    const action_id = approval_gate.requestApproval(
        alloc,
        redis,
        session.fleet_id,
        detail,
    ) catch |err| {
        // Redis unavailable — fail closed (default-deny). Surface the registry
        // code so operators can trace the default-deny back to gate-service loss.
        log.warn("gate_redis_unavailable", .{ .fleet_id = session.fleet_id, .event_id = event.event_id, .error_code = error_codes.ERR_APPROVAL_REDIS_UNAVAILABLE, .err = @errorName(err) });
        logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_DENIED, "gate_unavailable");
        return .unavailable;
    };
    defer alloc.free(action_id);

    logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_REQUIRED, action_id);
    const slack_msg = approval_gate.buildSlackApprovalMessage(
        alloc,
        session.config.name,
        action_id,
        detail,
        "", // callback_url resolved at delivery time by the notification provider
    ) catch |err| {
        log.warn("slack_msg_build_fail", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return .unavailable;
    };
    defer alloc.free(slack_msg);

    // Store the notification payload in Redis for the provider to pick up
    storeNotificationPayload(redis, session.fleet_id, action_id, slack_msg);

    approval_gate.recordGatePending(
        pool,
        alloc,
        session.fleet_id,
        session.workspace_id,
        action_id,
        event.event_id,
        detail,
    );

    const deadline_ms = clock.nowMillis() + detail.timeout_ms;
    approval_gate_async.recordEventGateRef(redis, session.fleet_id, event.event_id, action_id, deadline_ms) catch |err| {
        // Without the ref the lease path could never resolve this gate —
        // fail toward unavailable like the requestApproval failure above.
        log.warn("gate_ref_record_fail", .{ .error_code = error_codes.ERR_APPROVAL_REDIS_UNAVAILABLE, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
        return .unavailable;
    };

    log.debug("gate_pending", .{ .fleet_id = session.fleet_id, .event_id = event.event_id, .action_id = action_id });
    return .parked;
}

fn storeNotificationPayload(redis: *queue_redis.Client, fleet_id: []const u8, action_id: []const u8, payload: []const u8) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "fleet:gate:notify:{s}:{s}", .{
        fleet_id, action_id,
    }) catch return;
    redis.setEx(key, payload, gate_constants.GATE_PENDING_TTL_SECONDS) catch |err| {
        log.warn("notify_store_fail", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    };
}
