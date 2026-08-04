// Approval gate integration for the lease verb's pre-execution billing path.
//
// Checks anomaly counters and gate policy before a lease is issued. An action
// requiring approval parks the event as `pending` (the lease answers no-work)
// and every later poll re-evaluates the recorded gate ref — approval proceeds,
// denial blocks, deadline expiry resolves timed_out. No thread ever sleeps
// waiting for a human decision.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const fleet_config = @import("../fleet_runtime/config.zig");
const approval_gate = @import("../fleet_runtime/approval_gate.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const resolver = @import("../fleet_runtime/approval_gate_resolver.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const error_codes = @import("../errors/error_registry.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");
const config_gates = @import("../fleet_runtime/config_gates.zig");
const gate_detail = @import("approval_gate_detail.zig");
const FleetSession = @import("fleet_session.zig");
const logging = @import("log");

const log = logging.scoped(.fleet_event_loop_gate);

const BlockReason = enum { approval_denied, timeout, unavailable };
const AutoKillTrigger = enum { anomaly, policy };

const GateCheckResult = union(enum) {
    passed: void,
    /// A human decision is outstanding — the lease answers no-work and the
    /// next poll re-evaluates the recorded gate ref.
    pending: void,
    blocked: BlockReason,
    auto_killed: AutoKillTrigger,
};

/// Check the approval gate for an incoming event.
/// Returns .passed if execution should proceed; .pending while a human
/// decision is outstanding; .blocked or .auto_killed otherwise.
pub fn checkApprovalGate(
    alloc: Allocator,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
) GateCheckResult {
    const gates = session.config.gates orelse return .{ .passed = {} };

    // 1. Anomaly check (fast path — before approval)
    const anomaly = approval_gate.checkAnomaly(
        redis,
        session.fleet_id,
        event.event_type,
        event.actor,
        gates.anomaly_rules,
    );
    if (anomaly == .auto_kill) {
        logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_AUTO_KILL, event.event_id);
        pauseFleet(pool, redis, session.fleet_id);
        return .{ .auto_killed = .anomaly };
    }

    // 2. Gate evaluation — parsed context must be deinit'd to avoid leak
    var context_parsed = parseEventContext(alloc, event.request_json);
    defer if (context_parsed) |*p| p.deinit();
    const context: ?std.json.Value = if (context_parsed) |p| p.value else null;
    const decision = approval_gate.evaluateGate(
        gates,
        event.event_type,
        event.actor,
        context,
    );

    switch (decision) {
        .auto_approve => {
            return .{ .passed = {} };
        },
        .auto_kill => {
            logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_AUTO_KILL, event.event_id);
            pauseFleet(pool, redis, session.fleet_id);
            return .{ .auto_killed = .policy };
        },
        .requires_approval => {
            // The matched rule carries the workspace-authored approval copy that
            // the decision enum discards. Same traversal, so it cannot disagree
            // about which rule applied.
            const rule = approval_gate.matchRule(gates, event.event_type, event.actor, context);
            return handleApprovalFlow(alloc, session, event, pool, redis, gates, rule, context);
        },
    }
}

fn handleApprovalFlow(
    alloc: Allocator,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    gates: fleet_config.GatePolicy,
    rule: ?config_gates.GateRule,
    context: ?std.json.Value,
) GateCheckResult {
    // Re-encounter: this event already has a recorded gate — evaluate it.
    if (approval_gate_async.lookupEventGateRef(redis, session.fleet_id, event.event_id)) |maybe_ref| {
        if (maybe_ref) |ref| return evaluatePendingGate(alloc, session, pool, redis, &ref);
    } else |err| {
        // Redis blip: stay pending rather than re-notify or fail the gate.
        log.warn("gate_ref_lookup_fail", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
        return .{ .pending = {} };
    }
    return requestNewGate(alloc, session, event, pool, redis, gates, rule, context);
}

fn requestNewGate(
    alloc: Allocator,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    gates: fleet_config.GatePolicy,
    rule: ?config_gates.GateRule,
    context: ?std.json.Value,
) GateCheckResult {
    // Two sources, deliberately separated: the workspace-authored half from the
    // matched rule (statable as fact) and the model-authored half from the event
    // (rendered as an attributed claim). See `approval_gate_detail`.
    var built = gate_detail.build(alloc, event, rule, context, @intCast(gates.timeout_ms));
    defer built.deinit(alloc);
    const detail = built.detail;

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
        return .{ .blocked = .unavailable };
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
        return .{ .blocked = .unavailable };
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
        detail,
    );

    const deadline_ms = clock.nowMillis() + @as(i64, @intCast(gates.timeout_ms));
    approval_gate_async.recordEventGateRef(redis, session.fleet_id, event.event_id, action_id, deadline_ms) catch |err| {
        // Without the ref the lease path could never resolve this gate —
        // fail toward unavailable like the requestApproval failure above.
        log.warn("gate_ref_record_fail", .{ .error_code = error_codes.ERR_APPROVAL_REDIS_UNAVAILABLE, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
        return .{ .blocked = .unavailable };
    };

    log.debug("gate_pending", .{ .fleet_id = session.fleet_id, .event_id = event.event_id, .action_id = action_id });
    return .{ .pending = {} };
}

fn evaluatePendingGate(
    alloc: Allocator,
    session: *FleetSession,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    ref: *const approval_gate_async.EventGateRef,
) GateCheckResult {
    const eval = approval_gate_async.evaluateRef(redis, pool, ref, clock.nowMillis()) catch |err| {
        // Redis blip: a transient read failure must not deny an approved gate.
        log.warn("gate_decision_read_fail", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .err = @errorName(err) });
        return .{ .pending = {} };
    };
    switch (eval) {
        .approved => {
            logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_APPROVED, ref.actionId());
            return .{ .passed = {} };
        },
        .denied => {
            logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_DENIED, ref.actionId());
            return .{ .blocked = .approval_denied };
        },
        .expired => {
            logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_TIMEOUT, ref.actionId());
            // Attribution must be the canonical "system:timeout" string the
            // sweeper also writes (resolve() dedups whichever lands first).
            approval_gate.resolveGateDecision(pool, ref.actionId(), .timed_out, resolver.SYSTEM_TIMEOUT, "", std.heap.page_allocator);
            cleanupPendingKey(redis, session.fleet_id, ref.actionId());
            return .{ .blocked = .timeout };
        },
        .pending => return .{ .pending = {} },
    }
}

/// Best-effort gate-event log. Gate transitions currently emit structured
/// logs; durable terminal state lands in core.fleet_events via the worker's
/// terminal UPDATE.
fn logGateActivity(pool: *pg.Pool, alloc: Allocator, session: *FleetSession, event_type: []const u8, detail: []const u8) void {
    _ = pool;
    _ = alloc;
    log.debug("gate_event", .{ .fleet_id = session.fleet_id, .workspace_id = session.workspace_id, .type = event_type, .detail = detail });
}

fn pauseFleet(pool: *pg.Pool, redis: *queue_redis.Client, fleet_id: []const u8) void {
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.fleets SET status = 'paused', updated_at = $1 WHERE id = $2::uuid
    , .{ clock.nowMillis(), fleet_id }) catch |err| return log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    // A paused fleet leaves the candidate query's reach, so the poll-site
    // clear can never remove its readiness field — clear it here, after the
    // pause committed, the same discipline as the fleet-status PATCH path.
    // On a failed UPDATE the clear is skipped: the fleet is still active and
    // its mark still names live work.
    fleet_ready.forceClear(redis, fleet_id);
}

fn cleanupPendingKey(redis: *queue_redis.Client, fleet_id: []const u8, action_id: []const u8) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}", .{
        gate_constants.GATE_PENDING_KEY_PREFIX, fleet_id, action_id,
    }) catch return;
    var resp = redis.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(redis.alloc);
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

fn parseEventContext(alloc: Allocator, json: []const u8) ?std.json.Parsed(std.json.Value) {
    if (json.len <= 2) return null;
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch null;
}
