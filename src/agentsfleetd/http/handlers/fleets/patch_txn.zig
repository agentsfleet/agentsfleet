//! The fleet-PATCH transaction: `SELECT … FOR UPDATE` → `If-Match` check →
//! markdown reparse → FSM-gated `UPDATE`. Split from `patch.zig` (which owns
//! the HTTP layer) under the file-length cap.
//!
//! The txn locks exactly one `core.fleets` row → deadlock structurally
//! impossible (Invariant 10). Per-txn defensive timeouts: lock_timeout=5s,
//! statement_timeout=10s, idle_in_transaction_session_timeout=5s. PG `55P03`
//! (lock_timeout) surfaces as `.lock_timeout` so the handler can answer 503
//! retryable rather than a generic 500.

const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const ec = @import("../../../errors/error_registry.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const etag_mod = @import("../../etag.zig");
const get = @import("get.zig");
const patch_body = @import("patch_body.zig");

const log = logging.scoped(.fleet_api);

const PatchBody = patch_body.PatchBody;

/// PG SQLSTATE for a row-lock timeout — the one code this txn turns into a
/// deterministic outcome rather than a generic failure.
const SQLSTATE_LOCK_TIMEOUT = "55P03";

pub const Updated = struct {
    /// New `updated_at` — the config revision the caller echoes back.
    revision: i64,
    /// ETag over the post-update markdown; the editor holds it for its next save.
    etag: []const u8,
};

/// Every deterministic outcome of a fleet PATCH. Unexpected DB errors
/// propagate via `anyerror`; everything the handler maps to a specific HTTP
/// response lives here.
pub const TxnOutcome = union(enum) {
    updated: Updated,
    /// The caller's `If-Match` did not match the row; payload is the current tag.
    stale_etag: []const u8,
    not_found,
    invalid_transition,
    invalid_trigger_markdown,
    invalid_gate_condition,
    invalid_source_markdown,
    invalid_required_tags, // SKILL.md `tags:` outside placement bounds
    name_mismatch,
    lock_timeout,
};

/// The locked pre-update row. The markdown pair is duped (not borrowed)
/// because the `If-Match` comparison and the post-update ETag both outlive
/// the SELECT's result set.
const Snapshot = struct {
    name: []const u8,
    status: []const u8,
    source_markdown: []const u8,
    trigger_markdown: ?[]const u8,

    fn deinit(self: Snapshot, alloc: std.mem.Allocator) void {
        alloc.free(self.name);
        alloc.free(self.status);
        alloc.free(self.source_markdown);
        if (self.trigger_markdown) |t| alloc.free(t);
    }
};

pub fn patchFleetInTxn(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    fleet_id: []const u8,
    body: PatchBody,
    if_match: ?[]const u8,
) anyerror!TxnOutcome {
    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    defer if (tx_open) {
        conn.rollback() catch |err| log.warn("rollback_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    };

    _ = conn.exec("SET LOCAL lock_timeout = '5s'", .{}) catch |err| return timeoutOutcomeOr(conn, err);
    _ = conn.exec("SET LOCAL statement_timeout = '10s'", .{}) catch |err| return timeoutOutcomeOr(conn, err);
    _ = conn.exec("SET LOCAL idle_in_transaction_session_timeout = '5s'", .{}) catch |err| return timeoutOutcomeOr(conn, err);

    // SELECT FOR UPDATE — acquire row-lock + snapshot. Every field is needed:
    // `name` for the SKILL.md/TRIGGER.md cross-file name invariant; `status`
    // for distinguishing 404 (terminal/killed) from 409 (FSM rejects) when the
    // UPDATE returns 0 rows; the markdown pair for the `If-Match` comparison
    // and the post-update ETag (a status-only PATCH still answers with the
    // source's current tag).
    const snapshot = snapshotForUpdate(alloc, conn, workspace_id, fleet_id) catch |err| {
        if (err == error.LockTimeout) return .{ .lock_timeout = {} };
        return err;
    };
    const current = snapshot orelse return .{ .not_found = {} };
    defer current.deinit(alloc);

    if (try etag_mod.staleTag(alloc, if_match, &get.sourceSurface(current.source_markdown, current.trigger_markdown))) |current_tag| {
        return .{ .stale_etag = current_tag };
    }

    // Reparse body markdown if present. ParsedTrigger / SkillMetadata own
    // heap; deinit'd on every return path (success and error).
    var parsed_trigger: ?fleet_config.ParsedTrigger = null;
    defer if (parsed_trigger) |*pt| pt.deinit(alloc);
    if (body.trigger_markdown) |tm| {
        parsed_trigger = fleet_config.parseTriggerMarkdownWithJson(alloc, tm) catch return .{ .invalid_trigger_markdown = {} };
    }

    var skill_meta: ?fleet_config.SkillMetadata = null;
    defer if (skill_meta) |*sm| sm.deinit(alloc);
    // Re-derived placement tags when source_markdown is reparsed; NULL ⇒ the
    // UPDATE's COALESCE keeps the existing required_tags. Borrowed from skill_meta
    // (alive for the txn) and passed straight through as a TEXT[] param.
    var new_required_tags: ?[]const []const u8 = null;
    if (body.source_markdown) |sm| {
        skill_meta = fleet_config.parseSkillMetadata(alloc, sm) catch return .{ .invalid_source_markdown = {} };
        const target_name = if (parsed_trigger) |pt| pt.config.name else current.name;
        if (!std.mem.eql(u8, skill_meta.?.name, target_name)) return .{ .name_mismatch = {} };
        if (!fleet_config.validRequiredTags(skill_meta.?.tags)) return .{ .invalid_required_tags = {} };
        new_required_tags = skill_meta.?.tags;
    }

    const new_config_json: ?[]const u8 = if (parsed_trigger) |pt| pt.config_json else body.config_json;

    // Reject a malformed gate condition on the to-be-persisted config_json
    // (markdown or direct path); lenient parse, non-gate fall-through.
    if (new_config_json) |cj| {
        if (fleet_config.parseFleetConfig(alloc, cj)) |cfg| {
            defer cfg.deinit(alloc);
            if (cfg.gates) |g| if (fleet_config.firstInvalidGateCondition(g.rules) != null)
                return .{ .invalid_gate_condition = {} };
        } else |err| if (err == error.OutOfMemory) return err;
    }

    const new_name: ?[]const u8 = if (parsed_trigger) |pt| pt.config.name else null;

    const revision_opt = updateFleetRow(conn, workspace_id, fleet_id, body, new_config_json, new_name, new_required_tags) catch |err| {
        if (err == error.LockTimeout) return .{ .lock_timeout = {} };
        return err;
    };

    if (revision_opt == null) {
        // FSM/terminal guard rejected. Use the snapshot we already hold to
        // distinguish — no extra round-trip needed (we still own the lock).
        if (std.mem.eql(u8, current.status, fleet_config.FleetStatus.killed.toSlice())) return .{ .not_found = {} };
        return .{ .invalid_transition = {} };
    }

    // The UPDATE COALESCEs each markdown column, so the post-update source is
    // the body's value where it supplied one and the snapshot's otherwise.
    const post_etag = try etag_mod.compute(alloc, &get.sourceSurface(
        body.source_markdown orelse current.source_markdown,
        if (body.trigger_markdown) |tm| tm else current.trigger_markdown,
    ));
    errdefer alloc.free(post_etag);

    _ = try conn.exec("COMMIT", .{});
    tx_open = false;
    return .{ .updated = .{ .revision = revision_opt.?, .etag = post_etag } };
}

fn snapshotForUpdate(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    fleet_id: []const u8,
) anyerror!?Snapshot {
    var q = PgQuery.from(conn.query(sql.SELECT_FLEET_FOR_UPDATE, .{ fleet_id, workspace_id }) catch |err| return lockTimeoutOr(conn, err));
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const name = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(name);
    const status = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(status);
    const source_markdown = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(source_markdown);
    const trigger_raw = try row.get(?[]const u8, 3);
    const trigger_markdown: ?[]const u8 = if (trigger_raw) |t| try alloc.dupe(u8, t) else null;

    return .{
        .name = name,
        .status = status,
        .source_markdown = source_markdown,
        .trigger_markdown = trigger_markdown,
    };
}

fn updateFleetRow(
    conn: *pg.Conn,
    workspace_id: []const u8,
    fleet_id: []const u8,
    body: PatchBody,
    new_config_json: ?[]const u8,
    new_name: ?[]const u8,
    new_required_tags: ?[]const []const u8,
) anyerror!?i64 {
    const now_ms = clock.nowMillis();
    const active = fleet_config.FleetStatus.active.toSlice();
    const stopped = fleet_config.FleetStatus.stopped.toSlice();
    const killed = fleet_config.FleetStatus.killed.toSlice();
    const paused = fleet_config.FleetStatus.paused.toSlice();

    var upd_q = PgQuery.from(conn.query(sql.PATCH_FLEET, .{
        new_config_json,
        body.status,
        now_ms,
        fleet_id,
        workspace_id,
        killed,
        stopped,
        active,
        &[_][]const u8{ active, paused },
        &[_][]const u8{ stopped, paused },
        body.trigger_markdown,
        body.source_markdown,
        new_name,
        new_required_tags,
    }) catch |err| return lockTimeoutOr(conn, err));
    defer upd_q.deinit();
    if (try upd_q.next()) |row| return try row.get(i64, 0);
    return null;
}

/// The driver carries the SQLSTATE on `conn.err.?.code` after `error.PG`.
fn isLockTimeout(conn: *pg.Conn, err: anyerror) bool {
    if (err != error.PG) return false;
    const pg_err = conn.err orelse return false;
    return std.mem.eql(u8, pg_err.code, SQLSTATE_LOCK_TIMEOUT);
}

/// Query-helper arm: surface a lock timeout as a named error the txn maps
/// back to `.lock_timeout`; anything else propagates to the generic 500.
fn lockTimeoutOr(conn: *pg.Conn, err: anyerror) anyerror {
    return if (isLockTimeout(conn, err)) error.LockTimeout else err;
}

/// Txn-body arm: the same classification, returned directly as an outcome.
fn timeoutOutcomeOr(conn: *pg.Conn, err: anyerror) anyerror!TxnOutcome {
    if (isLockTimeout(conn, err)) return .{ .lock_timeout = {} };
    return err;
}
