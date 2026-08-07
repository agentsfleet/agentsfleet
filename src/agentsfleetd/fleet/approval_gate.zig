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
const gate_route = @import("approval_gate_route.zig");
const park = @import("approval_gate_park.zig");
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

/// Outcome of the pre-policy recorded-gate lookup. `unreadable` stays distinct
/// from `absent` because collapsing them is unsafe in both directions: an absent
/// ref means this event was never parked, while an unreadable one means we
/// cannot tell — and raising a SECOND approval card for an event that may
/// already hold one is worse than waiting a poll.
const RefLookup = union(enum) {
    found: approval_gate_async.EventGateRef,
    absent,
    unreadable,

    fn state(self: @This()) gate_route.RefState {
        return switch (self) {
            .found => .found,
            .absent => .absent,
            .unreadable => .unreadable,
        };
    }
};

fn lookupGateRef(
    redis: *queue_redis.Client,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
) RefLookup {
    const maybe_ref = approval_gate_async.lookupEventGateRef(redis, session.fleet_id, event.event_id) catch |err| {
        log.warn("gate_ref_lookup_fail", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
        return .unreadable;
    };
    return if (maybe_ref) |ref| .{ .found = ref } else .absent;
}

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
    // A recorded gate ref means this event was ALREADY parked and a human was
    // already asked. That question outlives the policy that raised it, so the ref
    // is read BEFORE any policy is consulted.
    //
    // Reading policy first let a mid-flight `config_json` PATCH silently withdraw
    // a question already put to a human: dropping `gates` returned .passed at the
    // top, and emptying `gates.rules` fell through to `.auto_approve` — either
    // way the parked event executed while its approval card still sat unanswered
    // in Slack. Waking a fleet and reconfiguring one are ONE scope today
    // (`fleet:write`), so that PATCH asks for no approval of its own; splitting
    // `fleet:message` out of it is its own piece of work, but honouring a gate
    // this daemon already raised does not have to wait for that.
    //
    // Cost: one Redis GET per event, ungated fleets included. It is bought on the
    // path that issues a lease — a whole model run — so it does not register.
    const lookup = lookupGateRef(redis, session, event);
    switch (lookup) {
        // Re-encounter: the recorded gate decides, whatever policy now says.
        .found => |*ref| return evaluatePendingGate(alloc, session, pool, redis, ref),
        .absent, .unreadable => {},
    }

    // KIND-PARK: a fleet whose repository binding declares WRITE
    // access parks every first-encounter event — before the rules walk AND
    // before the no-gates return below. Gate rules cannot hold this boundary:
    // `.auto_approve` is their no-match fallthrough and rules ride
    // `config_json`, PATCHable under the same `fleet:write` scope that wakes
    // the fleet. Anomaly counters are skipped on this path: each event costs
    // one card and executes nothing until a human answers, so the human is the
    // runaway brake here.
    if (writeAccess(session.config)) return parkWriteKind(alloc, session, event, pool, redis, lookup.state());

    const gates = session.config.gates orelse return .{ .passed = {} };

    // 1. Anomaly check (fast path — before approval). Reached only on a FIRST
    // encounter: the counter is an INCR, so re-polling a parked event through it
    // would count one waiting human as N runaway attempts and eventually
    // auto-kill the fleet for being patient.
    const anomaly = approval_gate.checkAnomaly(
        redis,
        session.fleet_id,
        event.event_type,
        event.actor,
        gates.anomaly_rules,
    );
    if (anomaly == .auto_kill) {
        park.logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_AUTO_KILL, event.event_id);
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

    return switch (gate_route.route(lookup.state(), decision)) {
        // Returned above, before any policy was read.
        .evaluate_recorded => unreachable,
        .pass => .{ .passed = {} },
        .kill => blk: {
            park.logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_AUTO_KILL, event.event_id);
            pauseFleet(pool, redis, session.fleet_id);
            break :blk .{ .auto_killed = .policy };
        },
        // An unreadable ref must not become a SECOND card for this event: wait a
        // poll rather than re-notify a human who may already hold one.
        .wait => .{ .pending = {} },
        .request_new => blk: {
            // The matched rule carries the workspace-authored approval copy that
            // the decision enum discards. Same traversal, so it cannot disagree
            // about which rule applied.
            const rule = approval_gate.matchRule(gates, event.event_type, event.actor, context);
            break :blk requestNewGate(alloc, session, event, pool, redis, gates, rule, context);
        },
    };
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
    // The binding comes from the fleet's own config, never from the event — it is
    // the same value the GitHub mint scopes the token by, so the card can state
    // the run's reach as fact while the model's claim about it stays a claim.
    var built = gate_detail.build(alloc, event, rule, context, @intCast(gates.timeout_ms), session.config.repository_binding);
    defer built.deinit(alloc);
    return parkOutcomeToResult(park.parkEvent(alloc, session, event, pool, redis, built.detail));
}

/// True when the fleet's repository egress binding declares write access — the
/// kind that parks unconditionally.
fn writeAccess(config: fleet_config.FleetConfig) bool {
    const binding = config.repository_binding orelse return false;
    return binding.access == .write;
}

/// Park a write-fleet event behind the write-kind card. `.unreadable` waits a
/// poll rather than raising a possible SECOND card for the same event — the
/// same discipline as the rules path's `.wait` route.
fn parkWriteKind(
    alloc: Allocator,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    ref_state: gate_route.RefState,
) GateCheckResult {
    if (ref_state == .unreadable) return .{ .pending = {} };
    var context_parsed = parseEventContext(alloc, event.request_json);
    defer if (context_parsed) |*p| p.deinit();
    const context: ?std.json.Value = if (context_parsed) |p| p.value else null;
    // No rule carries workspace copy here, so the kind and radius are the
    // daemon's own constants; the timeout is the gate default rather than a
    // policy value a PATCH could stretch.
    var built = gate_detail.build(alloc, event, null, context, @intCast(gate_constants.GATE_DEFAULT_TIMEOUT_MS), session.config.repository_binding);
    defer built.deinit(alloc);
    built.detail.gate_kind = gate_constants.GATE_KIND_REPOSITORY_WRITE;
    built.detail.blast_radius = gate_constants.GATE_BLAST_RADIUS_REPOSITORY_WRITE;
    return parkOutcomeToResult(park.parkEvent(alloc, session, event, pool, redis, built.detail));
}

fn parkOutcomeToResult(outcome: park.ParkOutcome) GateCheckResult {
    return switch (outcome) {
        .parked => .{ .pending = {} },
        .unavailable => .{ .blocked = .unavailable },
    };
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
            park.logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_APPROVED, ref.actionId());
            return .{ .passed = {} };
        },
        .denied => {
            park.logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_DENIED, ref.actionId());
            return .{ .blocked = .approval_denied };
        },
        .expired => {
            park.logGateActivity(pool, alloc, session, gate_constants.GATE_EVENT_TIMEOUT, ref.actionId());
            // Attribution must be the canonical "system:timeout" string the
            // sweeper also writes (resolve() dedups whichever lands first).
            approval_gate.resolveGateDecision(pool, ref.actionId(), .timed_out, resolver.SYSTEM_TIMEOUT, "", std.heap.page_allocator);
            cleanupPendingKey(redis, session.fleet_id, ref.actionId());
            return .{ .blocked = .timeout };
        },
        .pending => return .{ .pending = {} },
    }
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

fn parseEventContext(alloc: Allocator, json: []const u8) ?std.json.Parsed(std.json.Value) {
    if (json.len <= 2) return null;
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch null;
}
