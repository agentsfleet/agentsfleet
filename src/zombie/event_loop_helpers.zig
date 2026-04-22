// Zombie event loop helpers — extracted per RULE FLL (350-line gate).
//
// Session checkpoint, credential resolution, context update, truncation, backoff,
// execution tracking (M23_001), steer poll (M23_001), sandbox execution.
// Called by event_loop.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const error_codes = @import("../errors/error_registry.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const backoff = @import("../reliability/backoff.zig");
const activity_stream = @import("activity_stream.zig");
const metrics_counters = @import("../observability/metrics_counters.zig");
const metrics_workspace = @import("../observability/metrics_workspace.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_consts = @import("../queue/constants.zig");
const executor_client = @import("../executor/client.zig");
const id_format = @import("../types/id_format.zig");

const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;

const log = std.log.scoped(.zombie_event_loop);

/// Iterate session.config.credentials and return the first successfully
/// resolved value from vault.secrets. Credentials are plain names (e.g. "agentmail").
pub fn resolveFirstCredential(alloc: Allocator, pool: *pg.Pool, session: *ZombieSession) ![]const u8 {
    for (session.config.credentials) |cred_name| {
        return resolveCredential(alloc, pool, session.workspace_id, cred_name) catch continue;
    }
    return error.CredentialNotFound;
}

/// M2_001: Resolve a zombie credential from vault.secrets via crypto_store.
/// Key naming: "zombie:{name}" in vault.secrets. Returns decrypted value.
fn resolveCredential(alloc: Allocator, pool: *pg.Pool, workspace_id: []const u8, name: []const u8) ![]const u8 {
    const key_name = try std.fmt.allocPrint(alloc, "zombie:{s}", .{name});
    defer alloc.free(key_name);

    const conn = try pool.acquire();
    defer pool.release(conn);
    return crypto_store.load(alloc, conn, workspace_id, key_name) catch |err| {
        log.warn("zombie_event_loop.credential_not_found workspace_id={s} name={s} error_code=" ++ error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING, .{ workspace_id, name });
        return err;
    };
}

pub fn loadSessionCheckpoint(alloc: Allocator, pool: *pg.Pool, zombie_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT context_json::text FROM core.zombie_sessions WHERE zombie_id = $1
    , .{zombie_id}));
    defer q.deinit();

    if (try q.next()) |row| {
        return try alloc.dupe(u8, try row.get([]const u8, 0));
    }
    return try alloc.dupe(u8, "{}");
}

pub fn updateSessionContext(
    alloc: Allocator,
    session: *ZombieSession,
    event_id: []const u8,
    agent_response: []const u8,
) !void {
    const truncated = truncateForJson(agent_response);

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
    while (end > 0 and (s[end] & 0xC0) == 0x80) end -= 1;
    return s[0..end];
}

pub fn sleepWithBackoff(cfg: EventLoopConfig, consecutive_errors: u32) void {
    const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
    const delay_ms = backoff.expBackoffJitter(
        if (consecutive_errors > 0) consecutive_errors - 1 else 0,
        cfg.poll_interval_ms,
        max_delay_ms,
    );
    var remaining = delay_ms;
    while (remaining > 0 and cfg.running.load(.acquire)) {
        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

// M15_002: exit_status label values for PostHog ZombieCompleted events.
pub const EXIT_PROCESSED = "processed";
pub const EXIT_AGENT_ERROR = "agent_error";
pub const EXIT_DELIVER_ERROR = "deliver_error";

/// M15_002: delivery bookkeeping — activity log + Prometheus + PostHog.
/// Called from event_loop.processEvent after deliverEvent resolves.
pub fn logDeliveryResult(
    cfg: EventLoopConfig,
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    stage_result: anytype,
    wall_ms: u64,
) void {
    const ok = stage_result.failure == null;
    if (ok) {
        log.info("zombie_event_loop.delivered zombie_id={s} event_id={s} tokens={d} wall_s={d}", .{
            session.zombie_id, event.event_id, stage_result.token_count, stage_result.wall_seconds,
        });
        activity_stream.logEvent(cfg.pool, alloc, .{
            .zombie_id = session.zombie_id,
            .workspace_id = session.workspace_id,
            .event_type = activity_stream.EVT_AGENT_RESPONSE,
            .detail = event.event_id,
        });
        metrics_counters.incZombiesCompleted();
        metrics_counters.addZombieTokens(stage_result.token_count);
        metrics_workspace.addTokens(session.workspace_id, session.zombie_id, stage_result.token_count);
        metrics_counters.observeZombieExecutionSeconds(wall_ms);
    } else {
        const label = stage_result.failure.?.label();
        log.warn("zombie_event_loop.agent_failure zombie_id={s} event_id={s} failure={s}", .{
            session.zombie_id, event.event_id, label,
        });
        activity_stream.logEvent(cfg.pool, alloc, .{
            .zombie_id = session.zombie_id,
            .workspace_id = session.workspace_id,
            .event_type = activity_stream.EVT_AGENT_ERROR,
            .detail = label,
        });
        metrics_counters.incZombiesFailed();
    }
    // Worker thread has no user context — distinct_id = workspace_id so events
    // group under the owning workspace (consistent with ZombieTriggered in webhooks).
    if (cfg.telemetry) |tel| {
        tel.capture(telemetry_mod.ZombieCompleted, .{
            .distinct_id = session.workspace_id,
            .workspace_id = session.workspace_id,
            .zombie_id = session.zombie_id,
            .event_id = event.event_id,
            .tokens = stage_result.token_count,
            .wall_ms = wall_ms,
            .exit_status = if (ok) EXIT_PROCESSED else EXIT_AGENT_ERROR,
            .time_to_first_token_ms = stage_result.time_to_first_token_ms,
        });
    }
}

/// M15_002: deliver-path failure (before stage_result is available).
pub fn recordDeliverError(cfg: EventLoopConfig, session: *ZombieSession, event_id: []const u8) void {
    metrics_counters.incZombiesFailed();
    if (cfg.telemetry) |tel| {
        tel.capture(telemetry_mod.ZombieCompleted, .{
            .distinct_id = session.workspace_id,
            .workspace_id = session.workspace_id,
            .zombie_id = session.zombie_id,
            .event_id = event_id,
            .tokens = 0,
            .wall_ms = 0,
            .exit_status = EXIT_DELIVER_ERROR,
        });
    }
}

// ── M23_001: Execution tracking ──────────────────────────────────────────────

/// Set active execution in session and DB. Non-fatal — tracking is observability only.
/// Called immediately after createExecution succeeds.
pub fn setExecutionActive(alloc: Allocator, session: *ZombieSession, execution_id: []const u8, pool: *pg.Pool) void {
    const owned = alloc.dupe(u8, execution_id) catch return;
    if (session.execution_id) |old| alloc.free(old);
    session.execution_id = owned;
    session.execution_started_at = std.time.milliTimestamp();
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.zombie_sessions
        \\SET execution_id = $1, execution_started_at = $2
        \\WHERE zombie_id = $3::uuid
    , .{ owned, session.execution_started_at, session.zombie_id }) catch {};
}

/// Clear active execution in session and DB. Non-fatal.
/// Called in defer after destroyExecution, and at claimZombie startup (crash recovery).
pub fn clearExecutionActive(alloc: Allocator, session: *ZombieSession, pool: *pg.Pool) void {
    if (session.execution_id) |old| {
        alloc.free(old);
        session.execution_id = null;
    }
    session.execution_started_at = 0;
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.zombie_sessions
        \\SET execution_id = NULL, execution_started_at = NULL
        \\WHERE zombie_id = $1::uuid
    , .{session.zombie_id}) catch {};
}

// ── M23_001: Steer poll ───────────────────────────────────────────────────────

/// Poll Redis for a pending steer signal and inject it as a synthetic event.
/// Called at the top of the runEventLoop while iteration, before pollNextEvent.
/// Non-fatal: errors are logged and discarded so they never stall the event loop.
pub fn pollSteerAndInject(alloc: Allocator, cfg: EventLoopConfig, session: *const ZombieSession) void {
    var key_buf: [128]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "zombie:{s}{s}", .{
        session.zombie_id, queue_consts.zombie_steer_key_suffix,
    }) catch return;

    const msg = cfg.redis.getDel(key) catch null orelse return;
    defer alloc.free(msg);

    const event_id = id_format.generateZombieId(alloc) catch {
        log.warn("zombie_event_loop.steer_poll_oom zombie_id={s}", .{session.zombie_id});
        return;
    };
    defer alloc.free(event_id);

    cfg.redis.xaddZombieEvent(session.zombie_id, event_id, "steer", "operator", msg) catch |err| {
        log.warn("zombie_event_loop.steer_inject_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
    };
    log.info("zombie_event_loop.steer_injected zombie_id={s} event_id={s}", .{ session.zombie_id, event_id });
}

// ── Sandbox execution (moved from event_loop.zig for RULE FLL) ───────────────

fn parseSessionContext(alloc: Allocator, json: []const u8) ?std.json.Value {
    if (json.len <= 2) return null;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch return null;
    return parsed.value;
}

/// Create executor session, run the stage, destroy session. Tracks execution_id
/// in session + DB for API visibility (M23_001).
pub fn executeInSandbox(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
) !executor_client.ExecutorClient.StageResult {
    const context_val = parseSessionContext(alloc, session.context_json);

    // trace_id and session_id both bind to event.event_id: no upstream trace
    // propagator exists yet, so the per-event ID doubles as both the distributed
    // trace handle and the per-turn session identifier.
    const execution_id = cfg.executor.createExecution(cfg.workspace_path, .{
        .trace_id = event.event_id,
        .zombie_id = session.zombie_id,
        .workspace_id = session.workspace_id,
        .session_id = event.event_id,
    }) catch |err| {
        log.err("zombie_event_loop.exec_create_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_SESSION_CREATE_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
    // clearExecutionActive defer runs BEFORE this (LIFO), so DB is cleared before socket is closed.
    defer {
        cfg.executor.destroyExecution(execution_id) catch {};
        alloc.free(execution_id);
    }

    // M23_001: track execution in session + DB so the steer API can read it.
    setExecutionActive(alloc, session, execution_id, cfg.pool);
    defer clearExecutionActive(alloc, session, cfg.pool);

    const api_key: []const u8 = resolveFirstCredential(alloc, cfg.pool, session) catch "";
    defer if (api_key.len > 0) alloc.free(api_key);

    return cfg.executor.startStage(execution_id, .{
        .agent_config = .{
            .system_prompt = session.instructions,
            .api_key = api_key,
        },
        .message = event.data_json,
        .context = context_val,
    }) catch |err| {
        log.err("zombie_event_loop.stage_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_STAGE_START_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
}
