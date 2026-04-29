// Worker write path: the 13-step processEvent body.
//
// Composes the existing event-loop helpers (gate check, executor stage,
// metering, session checkpoint) with three responsibilities specific to
// the streaming substrate: INSERT/UPDATE on core.zombie_events, PUBLISH
// event_received / event_complete on zombie:{id}:activity, and a real
// ProgressEmitter wired to the activity_publisher helpers so executor-
// streamed frames reach SSE consumers (executor-side frame production
// lands when the NullClaw lifecycle hooks ship — until then the emitter
// fires zero times and behaviour matches a one-shot startStage).
//
// The 13 steps follow the EXECUTE flow in docs/ARCHITECHTURE.md.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const executor_client = @import("../executor/client.zig");
const executor_transport = @import("../executor/transport.zig");
const progress_callbacks = @import("../executor/progress_callbacks.zig");
const id_format = @import("../types/id_format.zig");

const event_loop_gate = @import("event_loop_gate.zig");
const helpers = @import("event_loop_helpers.zig");
const metering = @import("metering.zig");
const activity_publisher = @import("activity_publisher.zig");
const obs_log = @import("../observability/logging.zig");
const error_codes = @import("../errors/error_registry.zig");

const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;

const log = std.log.scoped(.zombie_event_loop);

// ── failure_label values written to core.zombie_events on gate_blocked ──────

const LABEL_BALANCE_EXHAUSTED = "balance_exhausted";
const LABEL_APPROVAL_DENIED = "approval_denied";
const LABEL_APPROVAL_TIMEOUT = "approval_timeout";
const LABEL_APPROVAL_UNAVAILABLE = "approval_unavailable";
const LABEL_AUTO_KILL_ANOMALY = "auto_kill_anomaly";
const LABEL_AUTO_KILL_POLICY = "auto_kill_policy";

const STATUS_PROCESSED = "processed";
const STATUS_AGENT_ERROR = "agent_error";
const STATUS_GATE_BLOCKED = "gate_blocked";

// ── ProgressEmitter wiring → activity_publisher PUBLISH thunks ──────────────

const EmitterCtx = struct {
    redis: *queue_redis.Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
};

fn emitProgress(ctx_ptr: *anyopaque, frame: progress_callbacks.ProgressFrame) void {
    const ctx: *const EmitterCtx = @ptrCast(@alignCast(ctx_ptr));
    switch (frame) {
        .tool_call_started => |body| activity_publisher.publishToolCallStarted(
            ctx.redis,
            ctx.alloc,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.args_redacted,
        ),
        .agent_response_chunk => |body| activity_publisher.publishChunk(
            ctx.redis,
            ctx.alloc,
            ctx.zombie_id,
            ctx.event_id,
            body.text,
        ),
        .tool_call_completed => |body| activity_publisher.publishToolCallCompleted(
            ctx.redis,
            ctx.alloc,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.ms,
        ),
        .tool_call_progress => |body| activity_publisher.publishToolCallProgress(
            ctx.redis,
            ctx.alloc,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.elapsed_ms,
        ),
    }
}

// ── INSERT zombie_events (status=received) — idempotent on replay ───────────

fn insertReceivedRow(
    pool: *pg.Pool,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, 'received', $6::jsonb, $7, $7)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{
        session.zombie_id,
        event.event_id,
        session.workspace_id,
        event.actor,
        event.event_type,
        event.request_json,
        now_ms,
    });
}

// ── UPDATE zombie_events status=gate_blocked + PUBLISH + XACK ───────────────

fn markGateBlocked(
    alloc: Allocator,
    cfg: EventLoopConfig,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    failure_label: []const u8,
) void {
    const conn = cfg.pool.acquire() catch |err| {
        log.warn("zombie_event_loop.gate_blocked_acquire_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
        return;
    };
    defer cfg.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE core.zombie_events
        \\SET status = 'gate_blocked', failure_label = $3, updated_at = $4
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ session.zombie_id, event.event_id, failure_label, now_ms }) catch |err| {
        log.warn("zombie_event_loop.gate_blocked_update_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
    };
    activity_publisher.publishEventComplete(cfg.redis, alloc, session.zombie_id, event.event_id, STATUS_GATE_BLOCKED);
    redis_zombie.xackZombie(cfg.redis, session.zombie_id, event.event_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.gate_blocked_xack_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
    };
}

// ── UPDATE zombie_events terminal (processed | agent_error) ─────────────────

fn markTerminal(
    pool: *pg.Pool,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    stage: *const executor_client.ExecutorClient.StageResult,
    wall_ms: u64,
) void {
    const conn = pool.acquire() catch |err| {
        log.warn("zombie_event_loop.terminal_acquire_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
        return;
    };
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    const status_text: []const u8 = if (stage.exit_ok) STATUS_PROCESSED else STATUS_AGENT_ERROR;
    _ = conn.exec(
        \\UPDATE core.zombie_events
        \\SET status = $3, response_text = $4, tokens = $5, wall_ms = $6, updated_at = $7
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{
        session.zombie_id,
        event.event_id,
        status_text,
        stage.content,
        @as(i64, @intCast(stage.token_count)),
        @as(i64, @intCast(wall_ms)),
        now_ms,
    }) catch |err| {
        log.warn("zombie_event_loop.terminal_update_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
    };
}

// ── UPSERT core.zombie_sessions (idle bookmark) ─────────────────────────────

fn checkpointZombieSession(alloc: Allocator, pool: *pg.Pool, session: *const ZombieSession) !void {
    const row_id = try id_format.generateZombieId(alloc);
    defer alloc.free(row_id);
    const now_ms = std.time.milliTimestamp();
    const conn = try pool.acquire();
    defer pool.release(conn);
    _ = try conn.exec(
        \\INSERT INTO core.zombie_sessions (id, zombie_id, context_json, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $4, $4)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET context_json = EXCLUDED.context_json,
        \\      checkpoint_at = EXCLUDED.checkpoint_at,
        \\      updated_at = EXCLUDED.updated_at
    , .{ row_id, session.zombie_id, session.context_json, now_ms });
}

// ── Gate composition ────────────────────────────────────────────────────────

const GateOutcome = union(enum) {
    passed: void,
    blocked: []const u8, // failure_label
};

fn runGates(
    alloc: Allocator,
    cfg: EventLoopConfig,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
) GateOutcome {
    if (metering.shouldBlockDelivery(cfg.pool, alloc, session.workspace_id, session.zombie_id, cfg.balance_policy)) {
        return .{ .blocked = LABEL_BALANCE_EXHAUSTED };
    }
    const gate = event_loop_gate.checkApprovalGate(alloc, session, event, cfg.pool, cfg.redis);
    return switch (gate) {
        .passed => .{ .passed = {} },
        .blocked => |reason| .{ .blocked = switch (reason) {
            .approval_denied => LABEL_APPROVAL_DENIED,
            .timeout => LABEL_APPROVAL_TIMEOUT,
            .unavailable => LABEL_APPROVAL_UNAVAILABLE,
        } },
        .auto_killed => |trigger| .{ .blocked = switch (trigger) {
            .anomaly => LABEL_AUTO_KILL_ANOMALY,
            .policy => LABEL_AUTO_KILL_POLICY,
        } },
    };
}

// ── Post-execution finalize: steps 9 → 13 ───────────────────────────────────

fn finalize(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
    stage_result: *const executor_client.ExecutorClient.StageResult,
    epoch_wall_time_ms: i64,
    wall_ms: u64,
) void {
    markTerminal(cfg.pool, session, event, stage_result, wall_ms);
    metering.recordZombieDelivery(
        cfg.pool,
        alloc,
        session.workspace_id,
        session.zombie_id,
        event.event_id,
        stage_result.wall_seconds,
        stage_result.token_count,
        stage_result.time_to_first_token_ms,
        epoch_wall_time_ms,
        cfg.balance_policy,
    );
    helpers.updateSessionContext(alloc, session, event.event_id, stage_result.content) catch |err| {
        log.warn("zombie_event_loop.context_update_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
    };
    checkpointZombieSession(alloc, cfg.pool, session) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.checkpoint_fail zombie_id={s} error_code=" ++ error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED, .{session.zombie_id});
    };
    helpers.logDeliveryResult(cfg, alloc, session, event, stage_result, wall_ms);
    const status_text: []const u8 = if (stage_result.exit_ok) STATUS_PROCESSED else STATUS_AGENT_ERROR;
    activity_publisher.publishEventComplete(cfg.redis, alloc, session.zombie_id, event.event_id, status_text);
    redis_zombie.xackZombie(cfg.redis, session.zombie_id, event.event_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.xack_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
    };
}

// ── Orchestrator ────────────────────────────────────────────────────────────

/// Drive a single event through the 13-step write path. Returns the new
/// consecutive_errors count for the caller's loop.
pub fn run(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
    consecutive_errors: *u32,
) u32 {
    insertReceivedRow(cfg.pool, session, event) catch |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.received_insert_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
        helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
        return consecutive_errors.* + 1;
    };
    activity_publisher.publishEventReceived(cfg.redis, alloc, session.zombie_id, event.event_id, event.actor);

    switch (runGates(alloc, cfg, session, event)) {
        .passed => {},
        .blocked => |label| {
            markGateBlocked(alloc, cfg, session, event, label);
            return 0;
        },
    }

    var emitter_ctx = EmitterCtx{
        .redis = cfg.redis,
        .alloc = alloc,
        .zombie_id = session.zombie_id,
        .event_id = event.event_id,
    };
    const emitter = executor_transport.ProgressEmitter{
        .ctx = @ptrCast(&emitter_ctx),
        .emit_fn = emitProgress,
    };
    const epoch_wall_time_ms = std.time.milliTimestamp();
    const t_start = std.time.Instant.now() catch null;
    const stage_result = helpers.executeInSandbox(alloc, session, event, cfg, emitter) catch |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.deliver_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
        helpers.recordDeliverError(cfg, session, event.event_id);
        helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
        return consecutive_errors.* + 1;
    };
    defer alloc.free(stage_result.content);
    const wall_ms: u64 = if (t_start) |s| blk: {
        const now = std.time.Instant.now() catch break :blk 0;
        break :blk now.since(s) / std.time.ns_per_ms;
    } else 0;

    finalize(alloc, session, event, cfg, &stage_result, epoch_wall_time_ms, wall_ms);
    return 0;
}
