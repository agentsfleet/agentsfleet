//! Worker-side continuation orchestrator. Splits the count + classify +
//! enqueue/force_stop helpers out of `event_loop_writepath.zig` to keep both
//! files under the 350-line cap. The pure decision logic lives in
//! `continuation.zig` (no DB/Redis); this file is the wire-up that calls
//! Postgres for the per-chain continuation count and Redis for the
//! synthetic-event XADD.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const executor_client = @import("../executor/client.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const continuation = @import("continuation.zig");
const event_envelope = @import("event_envelope.zig");
const event_loop_types = @import("event_loop_types.zig");
const ZombieSession = event_loop_types.ZombieSession;
const EventLoopConfig = event_loop_types.EventLoopConfig;

const log = logging.scoped(.zombie_event_loop);

/// failure_label written to the originating event row when the
/// continuation chain hits its per-chain cap.
/// Operator-facing; surfaces in `zombiectl events`, the dashboard
/// Events tab, and the activity stream's terminal `event_complete`
/// frame. Notification today is silent — the label is observability
/// only; an active escalation channel is a follow-up scope item.
pub const LABEL_CHUNK_CHAIN_ESCALATE_HUMAN = "chunk_chain_escalate_human";

/// Count continuation events in the chain ending at `event_id`. Walks
/// `resumes_event_id` recursively. The current event's row is included
/// in the count when its own `event_type='continuation'` — i.e., if
/// stage N just finished and N is itself a continuation, the chain already
/// carries N continuations and the next would be N+1.
fn countPriorContinuationsForChain(
    pool: *pg.Pool,
    zombie_id: []const u8,
    event_id: []const u8,
) !u32 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\WITH RECURSIVE chain AS (
        \\    SELECT event_id, event_type, resumes_event_id
        \\      FROM core.zombie_events
        \\     WHERE zombie_id = $1::uuid AND event_id = $2
        \\    UNION ALL
        \\    SELECT e.event_id, e.event_type, e.resumes_event_id
        \\      FROM core.zombie_events e
        \\      JOIN chain c ON e.zombie_id = $1::uuid
        \\                  AND e.event_id = c.resumes_event_id
        \\)
        \\SELECT count(*) FROM chain WHERE event_type = 'continuation'
    , .{ zombie_id, event_id }));
    defer q.deinit();
    if (try q.next()) |row| {
        const n = try row.get(i64, 0);
        if (n <= 0) return 0;
        return @intCast(@min(n, std.math.maxInt(u32)));
    }
    return 0;
}

/// UPDATE the originating event row with the chunk-chain failure
/// label. Worker stops there — no XADD — and the operator sees the
/// label in the events history. Per-zombie scope: only this event's
/// chain is forfeit; the zombie itself keeps responding to fresh
/// triggers (steer, webhook, cron) on a new chain.
fn markChunkChainEscalate(
    pool: *pg.Pool,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
) void {
    const conn = pool.acquire() catch |err| {
        log.warn("escalate_acquire_fail", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .err = @errorName(err) });
        return;
    };
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE core.zombie_events
        \\   SET failure_label = $3, updated_at = $4
        \\ WHERE zombie_id = $1::uuid AND event_id = $2
    , .{
        session.zombie_id,
        event.event_id,
        LABEL_CHUNK_CHAIN_ESCALATE_HUMAN,
        now_ms,
    }) catch |err| {
        log.warn("escalate_update_fail", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .err = @errorName(err) });
    };
}

/// XADD a synthetic continuation event onto `zombie:{id}:events`. The
/// new event opens the next stage with a `memory_recall` of the
/// snapshot. Caller has already classified — this fn just builds the
/// envelope and dispatches.
fn enqueueContinuationEvent(
    alloc: Allocator,
    redis: *queue_redis.Client,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    checkpoint_id: []const u8,
) !void {
    const new_actor = try event_envelope.buildContinuationActor(alloc, event.actor);
    defer alloc.free(new_actor);
    const request_json = try continuation.buildContinuationRequestJson(alloc, checkpoint_id, event.event_id);
    defer alloc.free(request_json);

    const envelope = event_envelope{
        // event_id is assigned by Redis (XADD `*`); the field is unused
        // by encodeForXAdd but the struct requires a value.
        .event_id = "",
        .zombie_id = session.zombie_id,
        .workspace_id = session.workspace_id,
        .actor = new_actor,
        .event_type = .continuation,
        .request_json = request_json,
        .created_at = std.time.milliTimestamp(),
    };

    const new_event_id = try redis.xaddZombieEvent(envelope);
    defer alloc.free(new_event_id);
    log.info("continuation_enqueued", .{
        .zombie_id = session.zombie_id,
        .parent = event.event_id,
        .new = new_event_id,
        .checkpoint_id = checkpoint_id,
    });
}

/// Classify the finished stage and act on the verdict.
/// Called after `markTerminal` so the originating row already reflects
/// `agent_error` status; force-stop only adds `failure_label`.
pub fn run(
    alloc: Allocator,
    cfg: EventLoopConfig,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    stage_result: *const executor_client.ExecutorClient.StageResult,
) void {
    if (stage_result.exit_ok) return;
    const checkpoint_id = stage_result.checkpoint_id orelse return;
    if (checkpoint_id.len == 0) return;

    const prior_count = countPriorContinuationsForChain(cfg.pool, session.zombie_id, event.event_id) catch |err| {
        log.warn("continuation_count_fail", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .err = @errorName(err) });
        // Fail-safe: when we can't read the count, treat as already-at-cap
        // so we never extend a chain we can't measure. Operator sees the
        // escalate-human label and steps in.
        markChunkChainEscalate(cfg.pool, session, event);
        return;
    };

    const verdict = continuation.classify(stage_result.*, event.actor, event.event_id, prior_count);
    switch (verdict) {
        .no_continuation => {},
        .force_stop => |s| {
            log.warn("chunk_chain_escalate_human", .{
                .zombie_id = session.zombie_id,
                .event_id = event.event_id,
                .prior_count = s.prior_continuation_count,
            });
            markChunkChainEscalate(cfg.pool, session, event);
        },
        .enqueue => |e| {
            enqueueContinuationEvent(alloc, cfg.redis, session, event, e.checkpoint_id) catch |err| {
                log.err("continuation_enqueue_fail", .{
                    .zombie_id = session.zombie_id,
                    .parent = event.event_id,
                    .err = @errorName(err),
                });
                // Mirror the count-fail failsafe: no XADD means no
                // recovery, so surface an escalate-human label rather
                // than silently abandon the chain.
                markChunkChainEscalate(cfg.pool, session, event);
            };
        },
    }
}
