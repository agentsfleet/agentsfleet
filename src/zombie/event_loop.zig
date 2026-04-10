// Zombie event loop — persistent agent process for a single Zombie.
//
// Lifecycle:
//   1. claimZombie — load config + session checkpoint from Postgres.
//   2. runEventLoop — XREADGROUP on zombie:{id}:events, deliver each event
//      to the NullClaw agent via the executor, checkpoint state, XACK.
//   3. On crash: next claimZombie loads the last checkpoint, resumes.
//
// Sequential mailbox ordering. At-least-once delivery (XACK after processing).
// Imports budget, kill switch, and cooperative shutdown from v1 shared primitives.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const zombie_config = @import("config.zig");
const activity_stream = @import("activity_stream.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_consts = @import("../queue/constants.zig");
const executor_client = @import("../executor/client.zig");
const executor_types = @import("../executor/types.zig");
const error_codes = @import("../errors/codes.zig");
const obs_log = @import("../observability/logging.zig");
const backoff = @import("../reliability/backoff.zig");
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.zombie_event_loop);

// ── Public types ─────────────────────────────────────────────────────────

pub const ZombieSession = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    config: zombie_config.ZombieConfig,
    instructions: []const u8,
    /// Session context (conversation memory) from core.zombie_sessions.
    /// JSON string. "{}" for a fresh session.
    context_json: []const u8,
    /// Source markdown — owns the memory that instructions borrows from.
    source_markdown: []const u8,

    pub fn deinit(self: *ZombieSession, alloc: Allocator) void {
        alloc.free(self.zombie_id);
        alloc.free(self.workspace_id);
        self.config.deinit(alloc);
        alloc.free(self.source_markdown);
        alloc.free(self.context_json);
    }
};

pub const EventResult = struct {
    status: Status,
    agent_response: []const u8,
    token_count: u64,
    wall_seconds: u64,

    pub const Status = enum { processed, skipped_duplicate, agent_error };

    pub fn deinit(self: *const EventResult, alloc: Allocator) void {
        alloc.free(self.agent_response);
    }
};

pub const EventLoopConfig = struct {
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    executor: *executor_client.ExecutorClient,
    /// Cooperative shutdown flag — checked between events.
    running: *const std.atomic.Value(bool),
    /// Poll interval for backoff on consecutive errors (ms).
    poll_interval_ms: u64 = 2_000,
    /// Executor socket path for createExecution workspace_path.
    workspace_path: []const u8 = "/tmp/zombie",
};

// ── Public API (spec §7.1) ───────────────────────────────────────────────

/// Claim a Zombie: load config + session checkpoint from Postgres.
/// Returns a ZombieSession that the caller owns and must deinit.
pub fn claimZombie(
    alloc: Allocator,
    zombie_id_input: []const u8,
    pool: *pg.Pool,
) !ZombieSession {
    // 1. Load zombie row from core.zombies
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = try conn.query(
        \\SELECT workspace_id::text, config_json::text, source_markdown, status
        \\FROM core.zombies WHERE id = $1
    , .{zombie_id_input}); // check-pg-drain: ok — drain in collectZombieRow
    defer q.deinit();

    const row = try q.next() orelse {
        log.warn("zombie_event_loop.claim_not_found zombie_id={s} error_code=" ++ error_codes.ERR_ZOMBIE_CLAIM_FAILED ++ " reason=not_found", .{zombie_id_input});
        return error.ZombieNotFound;
    };

    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const config_json = try alloc.dupe(u8, try row.get([]const u8, 1));
    defer alloc.free(config_json);
    const source_markdown = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(source_markdown);
    // Check status before drain — row-backed slices are invalid after drain.
    const is_active = std.mem.eql(u8, try row.get([]const u8, 3), error_codes.ZOMBIE_STATUS_ACTIVE);

    q.drain() catch {};

    if (!is_active) {
        log.warn("zombie_event_loop.claim_skipped zombie_id={s}", .{zombie_id_input});
        // errdefer on workspace_id and source_markdown fires automatically —
        // no manual free here (would be a double-free).
        return error.ZombieNotActive;
    }

    // 2. Parse config
    var config = try zombie_config.parseZombieConfig(alloc, config_json);
    errdefer config.deinit(alloc);

    // 3. Extract instructions (borrowed from source_markdown)
    const instructions = zombie_config.extractZombieInstructions(source_markdown);

    // 4. Load session checkpoint (or default to fresh)
    const context_json = try loadSessionCheckpoint(alloc, pool, zombie_id_input);
    errdefer alloc.free(context_json);

    const zombie_id = try alloc.dupe(u8, zombie_id_input);
    errdefer alloc.free(zombie_id);

    log.info("zombie_event_loop.claimed zombie_id={s} name={s} has_checkpoint={}", .{
        zombie_id, config.name, context_json.len > 2,
    });

    return ZombieSession{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .config = config,
        .instructions = instructions,
        .context_json = context_json,
        .source_markdown = source_markdown,
    };
}

/// Run the event loop for a claimed Zombie. Blocks until shutdown signal.
/// Reads events from Redis Streams, delivers to executor, checkpoints, XACKs.
pub fn runEventLoop(
    alloc: Allocator,
    session: *ZombieSession,
    cfg: EventLoopConfig,
) void {
    redis_zombie.ensureZombieConsumerGroup(cfg.redis, session.zombie_id) catch |err| {
        obs_log.logErrWithHint(.zombie_event_loop, err, error_codes.ERR_STARTUP_REDIS_GROUP, "zombie_event_loop.group_create_fail zombie_id={s}", .{session.zombie_id});
        return;
    };

    const consumer_id = queue_redis.makeConsumerId(alloc) catch "zombie-local";
    defer if (!std.mem.eql(u8, consumer_id, "zombie-local")) alloc.free(consumer_id);

    log.info("zombie_event_loop.started zombie_id={s} name={s}", .{ session.zombie_id, session.config.name });
    logActivity(cfg.pool, alloc, session, "zombie_started", "Event loop started");

    var last_reclaim_ms: i64 = std.time.milliTimestamp();
    var consecutive_errors: u32 = 0;

    while (cfg.running.load(.acquire)) {
        const poll_result = pollNextEvent(cfg, session, consumer_id, &last_reclaim_ms);
        if (poll_result.err) {
            consecutive_errors += 1;
            sleepWithBackoff(cfg, consecutive_errors);
            continue;
        }
        var evt = poll_result.event orelse {
            consecutive_errors = 0;
            continue;
        };
        defer evt.deinit(alloc);

        consecutive_errors = processEvent(alloc, session, &evt, cfg, &consecutive_errors);
        if (!cfg.running.load(.acquire)) break;
    }

    log.info("zombie_event_loop.stopped zombie_id={s}", .{session.zombie_id});
    logActivity(cfg.pool, alloc, session, "zombie_stopped", "Event loop stopped (shutdown signal)");
}

const PollResult = struct { event: ?redis_zombie.ZombieEvent, err: bool };

fn pollNextEvent(cfg: EventLoopConfig, session: *ZombieSession, consumer_id: []const u8, last_reclaim_ms: *i64) PollResult {
    const now_ms = std.time.milliTimestamp();
    if (now_ms - last_reclaim_ms.* >= queue_consts.zombie_reclaim_interval_ms) {
        last_reclaim_ms.* = now_ms;
        if (redis_zombie.xautoclaimZombie(cfg.redis, session.zombie_id, consumer_id)) |evt| {
            return .{ .event = evt, .err = false };
        } else |err| {
            obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.xautoclaim_fail zombie_id={s}", .{session.zombie_id});
            return .{ .event = null, .err = true };
        }
    }
    if (redis_zombie.xreadgroupZombie(cfg.redis, session.zombie_id, consumer_id)) |evt| {
        return .{ .event = evt, .err = false };
    } else |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.xreadgroup_fail zombie_id={s}", .{session.zombie_id});
        return .{ .event = null, .err = true };
    }
}

fn processEvent(alloc: Allocator, session: *ZombieSession, evt: *redis_zombie.ZombieEvent, cfg: EventLoopConfig, consecutive_errors: *u32) u32 {
    var result = deliverEvent(alloc, session, evt, cfg) catch |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.deliver_fail zombie_id={s} event_id={s}", .{ session.zombie_id, evt.event_id });
        logActivity(cfg.pool, alloc, session, "event_error", evt.event_id);
        sleepWithBackoff(cfg, consecutive_errors.* + 1);
        return consecutive_errors.* + 1;
    };
    defer result.deinit(alloc);

    checkpointState(alloc, session, cfg.pool) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.checkpoint_fail zombie_id={s} error_code=" ++ error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED, .{session.zombie_id});
        return consecutive_errors.* + 1;
    };

    redis_zombie.xackZombie(cfg.redis, session.zombie_id, evt.message_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.xack_fail zombie_id={s} message_id={s}", .{ session.zombie_id, evt.message_id });
    };
    return 0;
}

fn logActivity(pool: *pg.Pool, alloc: Allocator, session: *ZombieSession, event_type: []const u8, detail: []const u8) void {
    activity_stream.logEvent(pool, alloc, .{
        .zombie_id = session.zombie_id,
        .workspace_id = session.workspace_id,
        .event_type = event_type,
        .detail = detail,
    });
}

/// Deliver a single event to the executor agent.
pub fn deliverEvent(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
) !EventResult {
    log.info("zombie_event_loop.deliver zombie_id={s} event_id={s} type={s}", .{
        session.zombie_id, event.event_id, event.event_type,
    });
    logActivity(cfg.pool, alloc, session, "event_received", event.event_id);

    const stage_result = try executeInSandbox(alloc, session, event, cfg);

    const response_owned = try alloc.dupe(u8, stage_result.content);
    errdefer alloc.free(response_owned);

    updateSessionContext(alloc, session, event.event_id, stage_result.content) catch |err| {
        log.warn("zombie_event_loop.context_update_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
    };

    logDeliveryResult(cfg.pool, alloc, session, event, &stage_result);

    return EventResult{
        .status = if (stage_result.exit_ok) .processed else .agent_error,
        .agent_response = response_owned,
        .token_count = stage_result.token_count,
        .wall_seconds = stage_result.wall_seconds,
    };
}

fn parseSessionContext(alloc: Allocator, json: []const u8) ?std.json.Value {
    if (json.len <= 2) return null;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch return null;
    return parsed.value;
}

fn executeInSandbox(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
) !executor_client.StageResult {
    const context_val = parseSessionContext(alloc, session.context_json);

    const execution_id = cfg.executor.createExecution(cfg.workspace_path, .{
        .trace_id = event.event_id,
        .run_id = event.event_id,
        .workspace_id = session.workspace_id,
        .stage_id = "zombie",
        .role_id = "agent",
        .skill_id = session.config.name,
    }) catch |err| {
        log.err("zombie_event_loop.exec_create_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_SESSION_CREATE_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
    defer {
        cfg.executor.destroyExecution(execution_id) catch {};
        alloc.free(execution_id);
    }

    return cfg.executor.startStage(execution_id, .{
        .stage_id = "zombie",
        .role_id = "agent",
        .skill_id = session.config.name,
        .agent_config = .{ .system_prompt = session.instructions },
        .message = event.data_json,
        .context = context_val,
    }) catch |err| {
        log.err("zombie_event_loop.stage_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_STAGE_START_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
}

fn logDeliveryResult(pool: *pg.Pool, alloc: Allocator, session: *ZombieSession, event: *const redis_zombie.ZombieEvent, stage_result: anytype) void {
    if (stage_result.failure) |failure| {
        log.warn("zombie_event_loop.agent_failure zombie_id={s} event_id={s} failure={s}", .{
            session.zombie_id, event.event_id, failure.label(),
        });
        logActivity(pool, alloc, session, "agent_error", failure.label());
    } else {
        log.info("zombie_event_loop.delivered zombie_id={s} event_id={s} tokens={d} wall_s={d}", .{
            session.zombie_id, event.event_id, stage_result.token_count, stage_result.wall_seconds,
        });
        logActivity(pool, alloc, session, "agent_response", event.event_id);
    }
}

/// Checkpoint session state to Postgres (UPSERT on zombie_id).
/// Stores the current context_json so crash recovery can resume.
pub fn checkpointState(
    alloc: Allocator,
    session: *const ZombieSession,
    pool: *pg.Pool,
) !void {
    const session_id = try id_format.generateZombieId(alloc);
    defer alloc.free(session_id);

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
    , .{ session_id, session.zombie_id, session.context_json, now_ms });

    log.debug("zombie_event_loop.checkpointed zombie_id={s} context_len={d}", .{ session.zombie_id, session.context_json.len });
}

// ── Internal helpers ─────────────────────────────────────────────────────

fn loadSessionCheckpoint(alloc: Allocator, pool: *pg.Pool, zombie_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = try conn.query(
        \\SELECT context_json::text FROM core.zombie_sessions WHERE zombie_id = $1
    , .{zombie_id}); // check-pg-drain: ok — drain after next()
    defer q.deinit();

    if (try q.next()) |row| {
        const ctx = try alloc.dupe(u8, try row.get([]const u8, 0));
        q.drain() catch {};
        return ctx;
    }
    q.drain() catch {};
    return try alloc.dupe(u8, "{}");
}

pub fn updateSessionContext(
    alloc: Allocator,
    session: *ZombieSession,
    event_id: []const u8,
    agent_response: []const u8,
) !void {
    // Build a minimal context update with proper JSON escaping.
    // Future: richer conversation memory with sliding window.
    const truncated = truncateForJson(agent_response);

    // Use std.json.Stringify to properly escape strings (handles ", \, control chars).
    const ContextUpdate = struct {
        last_event_id: []const u8,
        last_response: []const u8,
    };

    const new_context = try std.json.Stringify.valueAlloc(alloc, ContextUpdate{
        .last_event_id = event_id,
        .last_response = truncated,
    }, .{});

    alloc.free(session.context_json);
    session.context_json = new_context;
}

/// Truncate a string for safe inclusion in a JSON value (max 2048 bytes).
/// Walks backward from the cut point to avoid splitting a multi-byte UTF-8 sequence.
pub fn truncateForJson(s: []const u8) []const u8 {
    const max_len: usize = 2048;
    if (s.len <= max_len) return s;
    var end = max_len;
    while (end > 0 and (s[end] & 0xC0) == 0x80) end -= 1; // skip continuation bytes
    return s[0..end];
}

fn sleepWithBackoff(cfg: EventLoopConfig, consecutive_errors: u32) void {
    const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
    const delay_ms = backoff.expBackoffJitter(
        if (consecutive_errors > 0) consecutive_errors - 1 else 0,
        cfg.poll_interval_ms,
        max_delay_ms,
    );
    // Cooperative sleep — check shutdown flag every 100ms
    var remaining = delay_ms;
    while (remaining > 0 and cfg.running.load(.acquire)) {
        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

test {
    _ = @import("event_loop_test.zig");
    _ = @import("event_loop_integration_test.zig");
}
