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
// Row-level helpers (insert/blocked/terminal/checkpoint) live in
// event_loop_writepath_rows.zig; tenant + provider resolution and the
// approval gate dispatch live in event_loop_writepath_resolve.zig.

const std = @import("std");
const Allocator = std.mem.Allocator;

const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const executor_transport = @import("../executor/transport.zig");
const progress_callbacks = @import("../executor/progress_callbacks.zig");

const event_loop_continuation = @import("event_loop_continuation.zig");
const helpers = @import("event_loop_helpers.zig");
const metering = @import("metering.zig");
const activity_publisher = @import("activity_publisher.zig");
const obs_log = @import("../observability/logging.zig");
const error_codes = @import("../errors/error_registry.zig");

const rows = @import("event_loop_writepath_rows.zig");
const resolve = @import("event_loop_writepath_resolve.zig");
const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;

const log = std.log.scoped(.zombie_event_loop);

// ── ProgressEmitter wiring → activity_publisher PUBLISH thunks ──────────────

const EmitterCtx = struct {
    redis: *queue_redis.Client,
    alloc: Allocator,
    scratch: *activity_publisher.Scratch,
    zombie_id: []const u8,
    event_id: []const u8,
};

fn emitProgress(ctx_ptr: *anyopaque, frame: progress_callbacks.ProgressFrame) void {
    const ctx: *EmitterCtx = @ptrCast(@alignCast(ctx_ptr));
    switch (frame) {
        .tool_call_started => |body| activity_publisher.publishToolCallStarted(
            ctx.redis,
            ctx.scratch,
            ctx.alloc,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.args_redacted,
        ),
        .agent_response_chunk => |body| activity_publisher.publishChunk(
            ctx.redis,
            ctx.scratch,
            ctx.zombie_id,
            ctx.event_id,
            body.text,
        ),
        .tool_call_completed => |body| activity_publisher.publishToolCallCompleted(
            ctx.redis,
            ctx.scratch,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.ms,
        ),
        .tool_call_progress => |body| activity_publisher.publishToolCallProgress(
            ctx.redis,
            ctx.scratch,
            ctx.zombie_id,
            ctx.event_id,
            body.name,
            body.elapsed_ms,
        ),
    }
}

// ── Post-execution finalize: steps 11 → 13 ──────────────────────────────────

fn finalize(
    alloc: Allocator,
    scratch: *activity_publisher.Scratch,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
    stage_result: *const @import("../executor/client.zig").ExecutorClient.StageResult,
    ctx: metering.PreflightContext,
    tenant_id: []const u8,
    epoch_wall_time_ms: i64,
    wall_ms: u64,
) void {
    rows.markTerminal(cfg.pool, session, event, stage_result, wall_ms);
    event_loop_continuation.run(alloc, cfg, session, event, stage_result);
    // The executor reports a single token_count (no input/output split yet);
    // attribute it all to output, mirroring the legacy convention. When the
    // executor surfaces the split, the call site here is the only place to
    // update.
    metering.recordStageActuals(
        cfg.pool,
        alloc,
        tenant_id,
        ctx,
        0,
        stage_result.token_count,
        wall_ms,
        epoch_wall_time_ms,
    );
    helpers.updateSessionContext(alloc, session, event.event_id, stage_result.content) catch |err| {
        log.warn("zombie_event_loop.context_update_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
    };
    rows.checkpointZombieSession(alloc, cfg.pool, session) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.checkpoint_fail zombie_id={s} error_code=" ++ error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED, .{session.zombie_id});
    };
    helpers.logDeliveryResult(cfg, alloc, session, event, stage_result, wall_ms);
    const status_text: []const u8 = if (stage_result.exit_ok) rows.STATUS_PROCESSED else rows.STATUS_AGENT_ERROR;
    activity_publisher.publishEventComplete(cfg.redis_publish, scratch, session.zombie_id, event.event_id, status_text);
    redis_zombie.xackZombie(cfg.redis, session.zombie_id, event.event_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.xack_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
    };
}

// ── Orchestrator ────────────────────────────────────────────────────────────

/// Drive a single event through the write path under the credit-pool model:
///
///   1. INSERT zombie_events status=received
///   2. Resolve tenant + provider (dead-letter on user-fixable BYOK error)
///   3. Balance gate (estimate = receive + stage at floor tokens)
///   4. RECEIVE debit + telemetry (transactional)
///   5. Approval gate
///   6. STAGE debit + telemetry (transactional, conservative estimate)
///   7. executor.startStage
///   8. UPDATE stage telemetry tokens + wall_ms; UPDATE zombie_events terminal;
///      checkpoint; PUBLISH event_complete; XACK
///
/// Returns the new consecutive_errors count for the caller's loop.
pub fn run(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
    consecutive_errors: *u32,
) u32 {
    rows.insertReceivedRow(alloc, cfg.pool, session, event) catch |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.received_insert_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
        helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
        return consecutive_errors.* + 1;
    };
    var scratch: activity_publisher.Scratch = .init(alloc);
    defer scratch.deinit();
    activity_publisher.publishEventReceived(cfg.redis_publish, &scratch, session.zombie_id, event.event_id, event.actor);

    // Resolver: load tenant_id + posture/model/api_key. The api_key isn't
    // consumed in this slice (executor still uses its own credential path),
    // but we need posture + model for the cost functions.
    var resolved_struct = switch (resolve.resolveTenantAndProvider(alloc, cfg, session, event.event_id)) {
        .resolved => |pair| pair,
        .dead_letter => |label| {
            rows.markBlocked(cfg, &scratch, session, event, rows.STATUS_DEAD_LETTERED, label);
            return 0;
        },
        .transient_err => {
            helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
            return consecutive_errors.* + 1;
        },
    };
    defer alloc.free(resolved_struct.tenant_id);
    defer resolved_struct.resolved.deinit(alloc);

    const ctx = metering.PreflightContext{
        .workspace_id = session.workspace_id,
        .zombie_id = session.zombie_id,
        .event_id = event.event_id,
        .posture = resolved_struct.resolved.mode,
        .model = resolved_struct.resolved.model,
    };

    if (!metering.balanceCoversEstimate(
        cfg.pool,
        alloc,
        resolved_struct.tenant_id,
        resolved_struct.resolved.mode,
        resolved_struct.resolved.model,
        cfg.balance_policy,
    )) {
        rows.markBlocked(cfg, &scratch, session, event, rows.STATUS_GATE_BLOCKED, rows.LABEL_BALANCE_EXHAUSTED);
        return 0;
    }

    switch (metering.debitReceive(cfg.pool, alloc, resolved_struct.tenant_id, ctx, cfg.balance_policy)) {
        .deducted => {},
        .exhausted => {
            rows.markBlocked(cfg, &scratch, session, event, rows.STATUS_GATE_BLOCKED, rows.LABEL_BALANCE_EXHAUSTED);
            return 0;
        },
        .missing_tenant_billing, .db_error => {
            helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
            return consecutive_errors.* + 1;
        },
    }

    switch (resolve.checkApprovalOnly(alloc, cfg, session, event)) {
        .passed => {},
        .blocked => |label| {
            rows.markBlocked(cfg, &scratch, session, event, rows.STATUS_GATE_BLOCKED, label);
            return 0;
        },
    }

    switch (metering.debitStage(cfg.pool, alloc, resolved_struct.tenant_id, ctx, cfg.balance_policy)) {
        .deducted => {},
        .exhausted => {
            // Per design: receive cents are NOT refunded on a stage-side
            // exhaust. The receive row stays committed; the event is
            // gate_blocked at the stage step.
            rows.markBlocked(cfg, &scratch, session, event, rows.STATUS_GATE_BLOCKED, rows.LABEL_BALANCE_EXHAUSTED);
            return 0;
        },
        .missing_tenant_billing, .db_error => {
            helpers.sleepWithBackoff(cfg, consecutive_errors.* + 1);
            return consecutive_errors.* + 1;
        },
    }

    var emitter_ctx = EmitterCtx{
        .redis = cfg.redis_publish,
        .alloc = alloc,
        .scratch = &scratch,
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

    finalize(alloc, &scratch, session, event, cfg, &stage_result, ctx, resolved_struct.tenant_id, epoch_wall_time_ms, wall_ms);
    return 0;
}
