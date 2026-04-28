// Zombie event loop — persistent agent process for a single Zombie.
// claimZombie loads config + session, runEventLoop reads events via XREADGROUP,
// delivers to executor, checkpoints state, XACKs. Sequential at-least-once delivery.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const zombie_config = @import("config.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_consts = @import("../queue/constants.zig");
const error_codes = @import("../errors/error_registry.zig");
const obs_log = @import("../observability/logging.zig");
const id_format = @import("../types/id_format.zig");
const event_loop_gate = @import("event_loop_gate.zig");
const metering = @import("metering.zig");

const log = std.log.scoped(.zombie_event_loop);

// Types + helpers extracted per RULE FLL (350-line gate).
const types = @import("event_loop_types.zig");
pub const ZombieSession = types.ZombieSession;
pub const EventResult = types.EventResult;
const helpers = @import("event_loop_helpers.zig");
pub const EventLoopConfig = types.EventLoopConfig;
const loadSessionCheckpoint = helpers.loadSessionCheckpoint;
pub const updateSessionContext = helpers.updateSessionContext;
const sleepWithBackoff = helpers.sleepWithBackoff;
pub const truncateForJson = helpers.truncateForJson;
const logDeliveryResult = helpers.logDeliveryResult;
const recordDeliverError = helpers.recordDeliverError;
// M23_001
const pollSteerAndInject = helpers.pollSteerAndInject;
const clearExecutionActive = helpers.clearExecutionActive;
pub const executeInSandbox = helpers.executeInSandbox;

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

    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, config_json::text, source_markdown, status
        \\FROM core.zombies WHERE id = $1
    , .{zombie_id_input}));
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
    // Check status before deinit — row-backed slices are invalid after deinit.
    const status = zombie_config.ZombieStatus.fromSlice(try row.get([]const u8, 3)) orelse .stopped;

    if (!status.isRunnable()) {
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

    var session = ZombieSession{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .config = config,
        .instructions = instructions,
        .context_json = context_json,
        .source_markdown = source_markdown,
    };
    // M23_001: clear any stale execution_id left by a crashed worker.
    clearExecutionActive(alloc, &session, pool);
    return session;
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

    var last_reclaim_ms: i64 = std.time.milliTimestamp();
    var consecutive_errors: u32 = 0;

    while (cfg.running.load(.acquire)) {
        // M23_001: inject any pending steer message into the event stream before polling.
        pollSteerAndInject(alloc, cfg, session);

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
    // M11_006 §3: pre-claim balance gate. Under policy `stop`, a tenant
    // whose billing row carries `balance_exhausted_at != NULL` never
    // reaches the executor — log + XACK so Redis doesn't retry.
    if (metering.shouldBlockDelivery(cfg.pool, alloc, session.workspace_id, session.zombie_id, cfg.balance_policy)) {
        redis_zombie.xackZombie(cfg.redis, session.zombie_id, evt.event_id) catch |err| {
            obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.xack_fail zombie_id={s} message_id={s}", .{ session.zombie_id, evt.event_id });
        };
        return 0;
    }

    var result = deliverEvent(alloc, session, evt, cfg) catch |err| {
        obs_log.logErr(.zombie_event_loop, err, "zombie_event_loop.deliver_fail zombie_id={s} event_id={s}", .{ session.zombie_id, evt.event_id });
        recordDeliverError(cfg, session, evt.event_id);
        sleepWithBackoff(cfg, consecutive_errors.* + 1);
        return consecutive_errors.* + 1;
    };
    defer result.deinit(alloc);

    checkpointState(alloc, session, cfg.pool) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.checkpoint_fail zombie_id={s} error_code=" ++ error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED, .{session.zombie_id});
        return consecutive_errors.* + 1;
    };
    metering.recordZombieDelivery(cfg.pool, alloc, session.workspace_id, session.zombie_id, evt.event_id, result.wall_seconds, result.token_count, result.time_to_first_token_ms, result.epoch_wall_time_ms, cfg.balance_policy);

    redis_zombie.xackZombie(cfg.redis, session.zombie_id, evt.event_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.xack_fail zombie_id={s} message_id={s}", .{ session.zombie_id, evt.event_id });
    };
    return 0;
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

    const epoch_wall_time_ms: i64 = std.time.milliTimestamp();
    // M15_002: wall-time spans the full deliver path — gate wait + sandbox — so the
    // histogram reflects end-to-end latency operators see, not just executor time.
    // Monotonic clock so system-clock steps don't skew or negate the duration.
    const t_start = std.time.Instant.now() catch null;

    // M4_001: Approval gate — check before tool execution
    const gate_check = event_loop_gate.checkApprovalGate(alloc, session, event, cfg.pool, cfg.redis);
    switch (gate_check) {
        .blocked => |reason| return EventResult{
            .status = .agent_error,
            .agent_response = try alloc.dupe(u8, switch (reason) {
                .approval_denied => "Action denied by operator",
                .timeout => "Approval timed out — action denied (default-deny)",
                .unavailable => "Gate service unavailable — action denied (default-deny)",
            }),
            .token_count = 0,
            .wall_seconds = 0,
        },
        .auto_killed => |trigger| return EventResult{
            .status = .agent_error,
            .agent_response = try alloc.dupe(u8, switch (trigger) {
                .anomaly => "Zombie auto-killed: anomaly pattern detected",
                .policy => "Zombie auto-killed: gate policy triggered",
            }),
            .token_count = 0,
            .wall_seconds = 0,
        },
        .passed => {},
    }

    const stage_result = try executeInSandbox(alloc, session, event, cfg);
    const wall_ms: u64 = if (t_start) |s| blk: {
        const now = std.time.Instant.now() catch break :blk 0;
        break :blk now.since(s) / std.time.ns_per_ms;
    } else 0;

    const response_owned = try alloc.dupe(u8, stage_result.content);
    errdefer alloc.free(response_owned);

    updateSessionContext(alloc, session, event.event_id, stage_result.content) catch |err| {
        log.warn("zombie_event_loop.context_update_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
    };

    logDeliveryResult(cfg, alloc, session, event, &stage_result, wall_ms);

    return EventResult{
        .status = if (stage_result.exit_ok) .processed else .agent_error,
        .agent_response = response_owned,
        .token_count = stage_result.token_count,
        .wall_seconds = stage_result.wall_seconds,
        .time_to_first_token_ms = stage_result.time_to_first_token_ms,
        .epoch_wall_time_ms = epoch_wall_time_ms,
    };
}

/// Checkpoint session state to Postgres (UPSERT on zombie_id).
/// Stores the current context_json so crash recovery can resume.
pub fn checkpointState(
    alloc: Allocator,
    session: *const ZombieSession,
    pool: *pg.Pool,
) !void {
    // row_id is the zombie_sessions PK — only consumed on INSERT; the ON CONFLICT
    // branch updates an existing row keyed by zombie_id and discards this id.
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

    log.debug("zombie_event_loop.checkpointed zombie_id={s} context_len={d}", .{ session.zombie_id, session.context_json.len });
}

test {
    _ = @import("event_loop_types.zig");
    _ = @import("event_loop_helpers.zig");
    _ = @import("event_loop_test.zig");
    _ = @import("event_loop_integration_test.zig");
}
