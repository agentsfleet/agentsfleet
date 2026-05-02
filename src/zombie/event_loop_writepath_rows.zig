// Row-level helpers for the worker write path: INSERT received, UPDATE
// blocked / terminal, UPSERT zombie_sessions checkpoint. Split out of
// event_loop_writepath.zig to keep both files under the 350-line cap.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const executor_client = @import("../executor/client.zig");
const id_format = @import("../types/id_format.zig");

const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;
const activity_publisher = @import("activity_publisher.zig");
const obs_log = @import("../observability/logging.zig");

const log = std.log.scoped(.zombie_event_loop);

// ── failure_label values written to core.zombie_events on gate_blocked ──────

pub const LABEL_BALANCE_EXHAUSTED = "balance_exhausted";
pub const LABEL_APPROVAL_DENIED = "approval_denied";
pub const LABEL_APPROVAL_TIMEOUT = "approval_timeout";
pub const LABEL_APPROVAL_UNAVAILABLE = "approval_unavailable";
pub const LABEL_AUTO_KILL_ANOMALY = "auto_kill_anomaly";
pub const LABEL_AUTO_KILL_POLICY = "auto_kill_policy";
pub const LABEL_PROVIDER_CREDENTIAL_MISSING = "provider_credential_missing";
pub const LABEL_PROVIDER_CREDENTIAL_MALFORMED = "provider_credential_malformed";

pub const STATUS_PROCESSED = "processed";
pub const STATUS_AGENT_ERROR = "agent_error";
pub const STATUS_GATE_BLOCKED = "gate_blocked";
pub const STATUS_DEAD_LETTERED = "dead_lettered";

pub fn insertReceivedRow(
    alloc: Allocator,
    pool: *pg.Pool,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();

    // Continuation events carry parent event_id in request_json's
    // `original_event_id` (§7); lift onto resumes_event_id for index walks.
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const resumes_event_id: ?[]const u8 = blk: {
        if (!std.mem.eql(u8, event.event_type, "continuation")) break :blk null;
        const parsed = std.json.parseFromSlice(std.json.Value, arena.allocator(), event.request_json, .{}) catch break :blk null;
        if (parsed.value != .object) break :blk null;
        const v = parsed.value.object.get("original_event_id") orelse break :blk null;
        break :blk if (v == .string) v.string else null;
    };

    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, resumes_event_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, 'received', $6::jsonb, $7, $8, $8)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{
        session.zombie_id,
        event.event_id,
        session.workspace_id,
        event.actor,
        event.event_type,
        event.request_json,
        resumes_event_id,
        now_ms,
    });
}

pub fn markBlocked(
    cfg: EventLoopConfig,
    scratch: *activity_publisher.Scratch,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    status_text: []const u8,
    failure_label: []const u8,
) void {
    const conn = cfg.pool.acquire() catch |err| {
        log.warn("zombie_event_loop.blocked_acquire_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
        return;
    };
    defer cfg.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE core.zombie_events
        \\SET status = $3, failure_label = $4, updated_at = $5
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ session.zombie_id, event.event_id, status_text, failure_label, now_ms }) catch |err| {
        log.warn("zombie_event_loop.blocked_update_fail zombie_id={s} event_id={s} err={s}", .{ session.zombie_id, event.event_id, @errorName(err) });
    };
    activity_publisher.publishEventComplete(cfg.redis_publish, scratch, session.zombie_id, event.event_id, status_text);
    redis_zombie.xackZombie(cfg.redis, session.zombie_id, event.event_id) catch |err| {
        obs_log.logWarnErr(.zombie_event_loop, err, "zombie_event_loop.blocked_xack_fail zombie_id={s} event_id={s}", .{ session.zombie_id, event.event_id });
    };
}

pub fn markTerminal(
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

pub fn checkpointZombieSession(alloc: Allocator, pool: *pg.Pool, session: *const ZombieSession) !void {
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
