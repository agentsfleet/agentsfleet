// Zombie event loop helpers — extracted per RULE FLL (350-line gate).
//
// Session checkpoint, credential resolution, context update, truncation, backoff.
// Called by event_loop.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const queue_redis = @import("../queue/redis_client.zig");
const executor_client = @import("../executor/client.zig");
const error_codes = @import("../errors/error_registry.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const backoff = @import("../reliability/backoff.zig");

const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;

const log = std.log.scoped(.zombie_event_loop);

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
