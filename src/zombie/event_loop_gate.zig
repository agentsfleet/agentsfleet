// M4_001: Approval gate integration for the event loop.
//
// Checks anomaly counters and gate policy before tool execution.
// If approval is required, blocks until human responds or timeout.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const zombie_config = @import("config.zig");
const approval_gate = @import("approval_gate.zig");
const activity_stream = @import("activity_stream.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const error_codes = @import("../errors/error_registry.zig");
const event_loop = @import("event_loop.zig");

const log = std.log.scoped(.zombie_event_loop_gate);

const BlockReason = enum { approval_denied, timeout, unavailable };
const AutoKillTrigger = enum { anomaly, policy };

const GateCheckResult = union(enum) {
    passed: void,
    blocked: BlockReason,
    auto_killed: AutoKillTrigger,
};

/// Check the approval gate for an incoming event.
/// Returns .passed if execution should proceed, .blocked or .auto_killed otherwise.
pub fn checkApprovalGate(
    alloc: Allocator,
    session: *event_loop.ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
) GateCheckResult {
    const gates = session.config.gates orelse return .{ .passed = {} };

    // 1. Anomaly check (fast path — before approval)
    const anomaly = approval_gate.checkAnomaly(
        redis,
        session.zombie_id,
        event.event_type,
        event.source,
        gates.anomaly_rules,
    );
    if (anomaly == .auto_kill) {
        logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_AUTO_KILL, event.event_id);
        pauseZombie(pool, session.zombie_id);
        return .{ .auto_killed = .anomaly };
    }

    // 2. Gate evaluation — parsed context must be deinit'd to avoid leak
    var context_parsed = parseEventContext(alloc, event.data_json);
    defer if (context_parsed) |*p| p.deinit();
    const context: ?std.json.Value = if (context_parsed) |p| p.value else null;
    const decision = approval_gate.evaluateGate(
        gates,
        event.event_type,
        event.source,
        context,
    );

    switch (decision) {
        .auto_approve => {
            return .{ .passed = {} };
        },
        .auto_kill => {
            logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_AUTO_KILL, event.event_id);
            pauseZombie(pool, session.zombie_id);
            return .{ .auto_killed = .policy };
        },
        .requires_approval => {
            return handleApprovalFlow(alloc, session, event, pool, redis, gates);
        },
    }
}

fn handleApprovalFlow(
    alloc: Allocator,
    session: *event_loop.ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    gates: zombie_config.GatePolicy,
) GateCheckResult {
    const action_id = approval_gate.requestApproval(
        alloc,
        redis,
        session.zombie_id,
        .{ .tool = event.event_type, .action = event.source, .params_summary = event.event_id },
    ) catch {
        // Redis unavailable — default-deny (spec section 6.0)
        logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_DENIED, "gate_unavailable");
        return .{ .blocked = .unavailable };
    };
    defer alloc.free(action_id);

    logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_REQUIRED, action_id);

    // Build and store the approval message (Slack/provider sends it via the skill tool).
    // The message payload is stored in Redis alongside the pending action so the
    // notification delivery layer (M8 Slack Plugin) can retrieve and POST it.
    const detail = approval_gate.ActionDetail{
        .tool = event.event_type,
        .action = event.source,
        .params_summary = event.event_id,
    };
    const slack_msg = approval_gate.buildSlackApprovalMessage(
        alloc,
        session.config.name,
        action_id,
        detail,
        "", // callback_url resolved at delivery time by the notification provider
    ) catch |err| {
        log.warn("approval_gate.slack_msg_build_fail err={s}", .{@errorName(err)});
        return .{ .blocked = .unavailable };
    };
    defer alloc.free(slack_msg);

    // Store the notification payload in Redis for the provider to pick up
    storeNotificationPayload(redis, session.zombie_id, action_id, slack_msg);

    approval_gate.recordGatePending(
        pool,
        alloc,
        session.zombie_id,
        session.workspace_id,
        action_id,
        event.event_type,
        event.source,
    );

    const result = approval_gate.waitForDecision(redis, action_id, gates.timeout_ms);
    return switch (result) {
        .approved => blk: {
            logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_APPROVED, action_id);
            break :blk .{ .passed = {} };
        },
        .denied => blk: {
            logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_DENIED, action_id);
            break :blk .{ .blocked = .approval_denied };
        },
        .timed_out => blk: {
            logGateActivity(pool, alloc, session, error_codes.GATE_EVENT_TIMEOUT, action_id);
            approval_gate.resolveGateDecision(pool, action_id, .timed_out, "");
            cleanupPendingKey(redis, session.zombie_id, action_id);
            break :blk .{ .blocked = .timeout };
        },
    };
}

fn logGateActivity(pool: *pg.Pool, alloc: Allocator, session: *event_loop.ZombieSession, event_type: []const u8, detail: []const u8) void {
    activity_stream.logEvent(pool, alloc, .{
        .zombie_id = session.zombie_id,
        .workspace_id = session.workspace_id,
        .event_type = event_type,
        .detail = detail,
    });
}

fn pauseZombie(pool: *pg.Pool, zombie_id: []const u8) void {
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.zombies SET status = 'paused', updated_at = $1 WHERE id = $2::uuid
    , .{ std.time.milliTimestamp(), zombie_id }) catch {};
}

fn cleanupPendingKey(redis: *queue_redis.Client, zombie_id: []const u8, action_id: []const u8) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}", .{
        error_codes.GATE_PENDING_KEY_PREFIX, zombie_id, action_id,
    }) catch return;
    var resp = redis.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(redis.alloc);
}

fn storeNotificationPayload(redis: *queue_redis.Client, zombie_id: []const u8, action_id: []const u8, payload: []const u8) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "zombie:gate:notify:{s}:{s}", .{
        zombie_id, action_id,
    }) catch return;
    redis.setEx(key, payload, error_codes.GATE_PENDING_TTL_SECONDS) catch |err| {
        log.warn("approval_gate.notify_store_fail err={s}", .{@errorName(err)});
    };
}

fn parseEventContext(alloc: Allocator, json: []const u8) ?std.json.Parsed(std.json.Value) {
    if (json.len <= 2) return null;
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch null;
}
