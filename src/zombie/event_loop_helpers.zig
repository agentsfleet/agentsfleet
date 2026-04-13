// Zombie event loop helpers — extracted per RULE FLL (350-line gate).
//
// Session checkpoint, credential resolution, context update, truncation, backoff.
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
const telemetry_mod = @import("../observability/telemetry.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");

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
