//! event_rows.zig — every durable `core.zombie_events` / `core.zombie_sessions`
//! write the runner control-plane verbs make: the received-row INSERT (lease),
//! the terminal-status UPDATE (report), and the session checkpoint UPSERT
//! (report).
//!
//! The received INSERT was lifted from the worker's `event_loop_writepath_rows`
//! at the M80 cutover — it keeps its `*ZombieSession` + `*ZombieEvent` params
//! because the lease verb has a real session + acquired event. The terminal +
//! checkpoint writers were narrowed to the few fields they read (the report
//! path has a `zombie_id` + `event_id` + `ExecutionResult`, never a full
//! `ZombieSession`), so the partial-struct/`undefined` shims the worker forced
//! are gone. Each write is best-effort + logged (non-atomic, mirroring the
//! deleted finalize); row-equivalence with the direct path is the invariant.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const id_format = @import("../types/id_format.zig");
const contract = @import("contract");
const logging = @import("log");
const redis_zombie = @import("../queue/redis_zombie.zig");
const ZombieSession = @import("zombie_session.zig");

const log = logging.scoped(.runner_report_rows);

const ExecutionResult = contract.execution_result.ExecutionResult;

/// `core.zombie_events.status` terminal values a runner report can produce
/// (app-enforced, no SQL CHECK — RULE STS). `gate_blocked`/`dead_lettered` are
/// zombied-side and never runner-reported.
pub const STATUS_PROCESSED = "processed";
pub const STATUS_AGENT_ERROR = "agent_error";

const EVENT_TYPE_CONTINUATION = "continuation";
const FIELD_ORIGINAL_EVENT_ID = "original_event_id";

/// INSERT the `received` event row at lease issue (the lease verb's first
/// durable write, mirroring the deleted worker's write path step 1). Keeps the
/// full `*ZombieSession` + `*ZombieEvent` because the lease has both. Idempotent
/// via the (zombie_id, event_id) PK.
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
        if (!std.mem.eql(u8, event.event_type, EVENT_TYPE_CONTINUATION)) break :blk null;
        const parsed = std.json.parseFromSlice(std.json.Value, arena.allocator(), event.request_json, .{}) catch break :blk null;
        if (parsed.value != .object) break :blk null;
        const v = parsed.value.object.get(FIELD_ORIGINAL_EVENT_ID) orelse break :blk null;
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

/// UPDATE the event row to its terminal status + response + telemetry + the
/// granular failure label. Reads `exit_ok`/`content`/`token_count`/`failure`
/// off the result; status is derived from `exit_ok`, and `failure_label` carries
/// the runner's `FailureClass` tag (NULL on a clean run, or a failure whose
/// reason the runner did not report). Best-effort (failures logged, not raised).
pub fn markTerminal(
    pool: *pg.Pool,
    zombie_id: []const u8,
    event_id: []const u8,
    result: ExecutionResult,
    wall_ms: u64,
) void {
    const conn = pool.acquire() catch |err| {
        log.warn("terminal_acquire_failed", .{ .zombie_id = zombie_id, .event_id = event_id, .err = @errorName(err) });
        return;
    };
    defer pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    const status_text: []const u8 = if (result.exit_ok) STATUS_PROCESSED else STATUS_AGENT_ERROR;
    const failure_label: ?[]const u8 = if (result.failure) |f| f.label() else null;
    _ = conn.exec(
        \\UPDATE core.zombie_events
        \\SET status = $3, response_text = $4, tokens = $5, wall_ms = $6, updated_at = $7, failure_label = $8
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{
        zombie_id,
        event_id,
        status_text,
        result.content,
        @as(i64, @intCast(result.token_count)),
        @as(i64, @intCast(wall_ms)),
        now_ms,
        failure_label,
    }) catch |err| {
        log.warn("terminal_update_failed", .{ .zombie_id = zombie_id, .event_id = event_id, .err = @errorName(err) });
    };
}

/// UPSERT the session resume cursor. Reads only `zombie_id` + the pre-built
/// `context_json` ({last_event_id, last_response}).
pub fn checkpointZombieSession(alloc: Allocator, pool: *pg.Pool, zombie_id: []const u8, context_json: []const u8) !void {
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
    , .{ row_id, zombie_id, context_json, now_ms });
}

/// Truncate a response to a JSON-safe length on a UTF-8 boundary so the
/// checkpoint's `last_response` is byte-identical to the direct path's.
pub fn truncateForJson(s: []const u8) []const u8 {
    const max_len: usize = 2048;
    if (s.len <= max_len) return s;
    var end = max_len;
    while (end > 0 and (s[end] & 0xC0) == 0x80) end -= 1;
    return s[0..end];
}

test "truncateForJson leaves short input untouched and caps long input on a UTF-8 boundary" {
    try std.testing.expectEqualStrings("hi", truncateForJson("hi"));
    const long = "x" ** 3000;
    const out = truncateForJson(long);
    try std.testing.expect(out.len <= 2048);
    try std.testing.expect(out.len >= 2048 - 4); // boundary walk-back is bounded
}
