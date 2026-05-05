// Zombie event loop — persistent agent process for a single Zombie.
// claimZombie loads config + session, runEventLoop reads events via XREADGROUP,
// delegates each event to event_loop_writepath.run for the 13-step write path.
// Sequential at-least-once delivery.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const zombie_config = @import("config.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const queue_consts = @import("../queue/constants.zig");
const error_codes = @import("../errors/error_registry.zig");
const obs_log = @import("../observability/logging.zig");

const log = std.log.scoped(.zombie_event_loop);

// Types + helpers extracted per RULE FLL (350-line gate).
const types = @import("event_loop_types.zig");
pub const ZombieSession = types.ZombieSession;
pub const EventResult = types.EventResult;
const helpers = @import("event_loop_helpers.zig");
pub const EventLoopConfig = types.EventLoopConfig;
const writepath = @import("event_loop_writepath.zig");
const loadSessionCheckpoint = helpers.loadSessionCheckpoint;
pub const updateSessionContext = helpers.updateSessionContext;
pub const truncateForJson = helpers.truncateForJson;
const sleepWithBackoff = helpers.sleepWithBackoff;
const clearExecutionActive = helpers.clearExecutionActive;

// ── Public API ───────────────────────────────────────────────────────────

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
    // Crash recovery: clear any stale execution_id left by a worker that
    // died mid-stage so the next createExecution starts from a clean slot.
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
        if (cfg.reload_pending) |flag| {
            if (flag.swap(false, .acq_rel)) {
                reloadZombieConfig(alloc, session, cfg.pool) catch |err| {
                    log.warn("zombie_event_loop.reload_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
                };
            }
        }
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
    return writepath.run(alloc, session, evt, cfg, consecutive_errors);
}

/// §9 hot-reload: re-read core.zombies.config_json, reparse, swap on
/// the session, free the old config. In-flight executions are not
/// interrupted (this only runs between events). Errors leave the old
/// config in place; the next reload signal can retry.
///
/// Public so the integration test in
/// `src/zombie/context_lifecycle_integration_test.zig` can drive the
/// reload without spinning the full runEventLoop (which blocks on
/// XREADGROUP). The runtime entry point stays in runEventLoop above.
pub fn reloadZombieConfig(alloc: Allocator, session: *ZombieSession, pool: *pg.Pool) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT config_json::text FROM core.zombies WHERE id = $1
    , .{session.zombie_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.ZombieNotFound;
    const config_json_owned = try alloc.dupe(u8, try row.get([]const u8, 0));
    defer alloc.free(config_json_owned);

    var new_config = try zombie_config.parseZombieConfig(alloc, config_json_owned);
    errdefer new_config.deinit(alloc);

    var old_config = session.config;
    session.config = new_config;
    old_config.deinit(alloc);
    log.info("zombie_event_loop.config_reloaded zombie_id={s} name={s}", .{ session.zombie_id, session.config.name });
}

test {
    _ = @import("event_loop_types.zig");
    _ = @import("event_loop_helpers.zig");
    _ = @import("event_loop_writepath.zig");
    _ = @import("event_loop_test.zig");
    _ = @import("event_loop_integration_test.zig");
    _ = @import("event_loop_writepath_integration_test.zig");
    _ = @import("test_executor_harness.zig");
    _ = @import("test_harness_helpers.zig");
    _ = @import("test_rpc_recorder.zig");
    _ = @import("event_loop_harness_integration_test.zig");
    _ = @import("event_loop_harness_heartbeat_test.zig");
    _ = @import("event_loop_harness_version_test.zig");
    _ = @import("event_loop_harness_recorder_test.zig");
    _ = @import("event_loop_lifecycle_test.zig");
}
