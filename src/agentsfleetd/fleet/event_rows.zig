//! event_rows.zig — every durable `core.fleet_events` / `core.fleet_sessions`
//! write the runner control-plane verbs make: the received-row INSERT (lease),
//! the terminal-status UPDATE (report), and the session checkpoint UPSERT
//! (report).
//!
//! The received INSERT was lifted from the worker's `event_loop_writepath_rows`
//! at the M80 cutover — it keeps its `*FleetSession` + `*FleetEvent` params
//! because the lease verb has a real session + acquired event. The terminal +
//! checkpoint writers were narrowed to the few fields they read (the report
//! path has a `fleet_id` + `event_id` + `ExecutionResult`, never a full
//! `FleetSession`), so the partial-struct/`undefined` shims the worker forced
//! are gone. Each write is best-effort + logged (non-atomic, mirroring the
//! deleted finalize); row-equivalence with the direct path is the invariant.

const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const contract = @import("contract");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const FleetSession = @import("fleet_session.zig");

const log = logging.scoped(.runner_report_rows);

const ExecutionResult = contract.execution_result.ExecutionResult;

/// `core.fleet_events.status` terminal values a runner report can produce
/// (app-enforced, no SQL CHECK — RULE STS). `gate_blocked`/`dead_lettered` are
/// agentsfleetd-side and never runner-reported.
pub const STATUS_PROCESSED = "processed";
pub const STATUS_FLEET_ERROR = "fleet_error";
/// Non-terminal ingress status; the guarded blocked-transition keys on it.
pub const STATUS_RECEIVED = "received";
/// agentsfleetd-side terminal status for lease-path gate refusals (scenario 03).
pub const STATUS_GATE_BLOCKED = "gate_blocked";

/// `failure_label` values for `gate_blocked` rows — single ownership site
/// (RULE UFS); webhook/steer/tests import these, never restate them.
/// `balance_exhausted` spelling is pinned by billing_and_provider_keys.md.
pub const LABEL_BALANCE_EXHAUSTED = "balance_exhausted";
pub const LABEL_TENANT_RESOLVE_FAILED = "tenant_resolve_failed";
pub const LABEL_SECRET_MISSING = "secret_missing";
pub const LABEL_APPROVAL_DENIED = "approval_denied";
pub const LABEL_APPROVAL_EXPIRED = "approval_expired";
/// The FLEET's own `daily_dollars`/`monthly_dollars` ceiling is reached. Spelt
/// identically to `contract.execution_result.FailureClass.budget_breach`, which
/// carries the same verdict for the mid-run kill — one label, two gates, so an
/// operator greps one string whether the run was refused or stopped.
pub const LABEL_BUDGET_BREACH = "budget_breach";

const EVENT_TYPE_CONTINUATION = "continuation";
const FIELD_ORIGINAL_EVENT_ID = "original_event_id";

/// Byte caps applied at the report write, on a UTF-8 boundary (`truncateUtf8`).
/// The cause line renders as one line in the console; Inspect shows the stored
/// value, so the cap bounds the row, not the operator's visibility.
pub const MAX_FAILURE_DETAIL_BYTES: usize = 512;
pub const MAX_CHECKPOINT_RESPONSE_BYTES: usize = 2048;

/// INSERT the `received` event row at lease issue (the lease verb's first
/// durable write, mirroring the deleted worker's write path step 1). Keeps the
/// full `*FleetSession` + `*FleetEvent` because the lease has both. Idempotent
/// via the (fleet_id, event_id) PK; returns false on the conflict no-op so the
/// caller can tell a re-delivered stream entry from a first delivery (and skip
/// the receive debit + the duplicate `event_received` frame).
pub fn insertReceivedRow(
    alloc: Allocator,
    pool: *pg.Pool,
    session: *FleetSession,
    event: *const redis_fleet.FleetEvent,
) !bool {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = clock.nowMillis();
    const uid_value = try id_format.generateUuidV7();
    const uid: []const u8 = &uid_value;

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

    const affected = try conn.exec(sql.INSERT_FLEET_EVENT, .{
        uid,
        session.fleet_id,
        event.event_id,
        session.workspace_id,
        event.actor,
        event.event_type,
        event.request_json,
        resumes_event_id,
        now_ms,
        STATUS_RECEIVED,
    });
    return (affected orelse 0) > 0;
}

/// UPDATE the event row to the `gate_blocked` terminal + the named failure
/// label. Guarded on `status = 'received'`: a terminal row is never reopened
/// (a re-request after gate_blocked is a NEW delivery — RULE IDMP). Errors
/// propagate so the caller withholds the XACK — the terminal write must commit
/// before the stream entry is acked, or the delivery would be lost. Returns
/// rows affected (0 = the row was already terminal; the XACK is still owed).
pub fn markBlocked(
    pool: *pg.Pool,
    fleet_id: []const u8,
    event_id: []const u8,
    failure_label: []const u8,
) !i64 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const affected = try conn.exec(sql.UPDATE_FLEET_EVENT_FAILURE, .{
        fleet_id,
        event_id,
        STATUS_GATE_BLOCKED,
        failure_label,
        clock.nowMillis(),
        STATUS_RECEIVED,
    });
    return affected orelse 0;
}

/// Status class of an existing event row, for the lease path's PEL re-delivery
/// branch. A re-delivery is a genuine re-poll only while the row is still
/// `received` (a pending-gate re-poll or a reclaimed strand); a `terminal` row
/// means a settled or `gate_blocked` entry whose XACK was lost — it must be
/// re-acked, never re-executed (spec Invariant 2). `absent` cannot follow a
/// conflicting insert but is treated as a proceed.
pub const RowClass = enum { absent, received, terminal };

pub fn classifyStatus(pool: *pg.Pool, fleet_id: []const u8, event_id: []const u8) !RowClass {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(sql.SELECT_FLEET_EVENT_STATUS, .{ fleet_id, event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return .absent;
    const status = try row.get([]const u8, 0);
    return if (std.mem.eql(u8, status, STATUS_RECEIVED)) .received else .terminal;
}

/// UPDATE the event row to its terminal status + response + telemetry + the
/// granular failure label. Reads the verdict/`content`/`token_count` off the
/// result; status is derived from the outcome, and `failure_label` carries
/// the runner's `FailureClass` tag (NULL on a clean run, or a failure whose
/// reason the runner did not report). Best-effort (failures logged, not raised).
pub fn markTerminal(
    pool: *pg.Pool,
    fleet_id: []const u8,
    event_id: []const u8,
    result: ExecutionResult,
    wall_ms: u64,
) void {
    const conn = pool.acquire() catch |err| {
        log.warn("terminal_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .event_id = event_id, .err = @errorName(err) });
        return;
    };
    defer pool.release(conn);
    const now_ms = clock.nowMillis();
    const status_text: []const u8 = if (result.succeeded()) STATUS_PROCESSED else STATUS_FLEET_ERROR;
    const failure = result.failure();
    const failure_label: ?[]const u8 = if (failure) |f| (if (f.class) |c| c.label() else null) else null;
    // The cause rides the failed variant, so it cannot accompany a clean run.
    // Capped at the write so a runaway child cannot bloat the row.
    const failure_detail: ?[]const u8 = if (failure) |f|
        (if (f.detail.len > 0) truncateUtf8(f.detail, MAX_FAILURE_DETAIL_BYTES) else null)
    else
        null;
    // Guarded on `status = 'received'`: a terminal row is never reopened
    // (spec Invariant 2 — same one-way-door discipline as markBlocked). The
    // happy path always transitions a single received→terminal; a 0-row write
    // means the row was already terminal (a re-delivery whose XACK was lost)
    // and is logged rather than silently overwriting the settled result.
    const affected = conn.exec(sql.UPDATE_FLEET_EVENT_RESULT, .{
        fleet_id,
        event_id,
        status_text,
        result.content,
        @as(i64, @intCast(result.token_count)),
        @as(i64, @intCast(wall_ms)),
        now_ms,
        failure_label,
        STATUS_RECEIVED,
        failure_detail,
    }) catch |err| {
        log.warn("terminal_update_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .event_id = event_id, .err = @errorName(err) });
        return;
    };
    if ((affected orelse 0) == 0) {
        log.warn("terminal_write_skipped_nonreceived", .{ .fleet_id = fleet_id, .event_id = event_id });
    }
}

/// UPSERT the session resume cursor. Reads only `fleet_id` + the pre-built
/// `context_json` ({last_event_id, last_response}).
pub fn checkpointFleetSession(alloc: Allocator, pool: *pg.Pool, fleet_id: []const u8, context_json: []const u8) !void {
    const row_id = try id_format.generateFleetId(alloc);
    defer alloc.free(row_id);
    const now_ms = clock.nowMillis();
    const conn = try pool.acquire();
    defer pool.release(conn);
    _ = try conn.exec(sql.UPSERT_FLEET_SESSION, .{ row_id, fleet_id, context_json, now_ms });
}

/// Truncate to `max_len` bytes on a UTF-8 boundary — no split code points, so
/// the value stays valid text for JSON emission and the console.
pub fn truncateUtf8(s: []const u8, max_len: usize) []const u8 {
    if (s.len <= max_len) return s;
    var end = max_len;
    while (end > 0 and (s[end] & 0xC0) == 0x80) end -= 1;
    return s[0..end];
}

test "truncateUtf8 leaves short input untouched and caps long input on a UTF-8 boundary" {
    try std.testing.expectEqualStrings("hi", truncateUtf8("hi", MAX_CHECKPOINT_RESPONSE_BYTES));
    const long = "x" ** 3000;
    const out = truncateUtf8(long, MAX_CHECKPOINT_RESPONSE_BYTES);
    try std.testing.expect(out.len <= MAX_CHECKPOINT_RESPONSE_BYTES);
    try std.testing.expect(out.len >= MAX_CHECKPOINT_RESPONSE_BYTES - 4); // boundary walk-back is bounded
    // The failure-cause cap follows the same boundary rule at its own limit.
    const capped = truncateUtf8(long, MAX_FAILURE_DETAIL_BYTES);
    try std.testing.expect(capped.len <= MAX_FAILURE_DETAIL_BYTES);
}
