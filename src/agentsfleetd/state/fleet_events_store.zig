//! Read-side store for `core.fleet_events` — operator-visible event
//! history. Two listing entry points:
//!
//!   listForFleet(...)    — per-fleet history.
//!   listForWorkspace(...) — workspace aggregate, optional fleet_id
//!                            filter for drill-down.
//!
//! Sorted newest-first by `(created_at DESC, event_id DESC)`. Filters
//! and cursor live in `fleet_events_filter.zig`.
//!
//! Caller-owned allocator: methods that allocate (incl. deinit) take
//! the allocator as a parameter.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const filter_mod = @import("fleet_events_filter.zig");

pub const Filter = filter_mod.Filter;
pub const makeCursor = filter_mod.makeCursor;
pub const parseSince = filter_mod.parseSince;
pub const SinceError = filter_mod.SinceError;
pub const globToLike = filter_mod.globToLike;
const parseCursor = filter_mod.parseCursor;

// SELECT list shared across every branch. Trailing newline matters —
// the concatenated suffix begins with WHERE/ORDER BY (PG syntax error
// 42601 if glued straight onto `core.fleet_events`).
//
// `cost_nanos` is what this event actually cost, summed over its telemetry
// rows — billing writes up to two per event (`receive`, `stage`, unique on
// `(event_id, charge_type)`), so a bare LEFT JOIN would duplicate the event
// row per leg. The correlated subselect keeps one row per event and yields
// SQL NULL when no telemetry exists (an unbilled run reads as "unknown", never
// as zero). Cost is server truth: the client never derives it from tokens.
//
// Perf note: this subselect runs once per returned event row (≤200/page), but
// the `UNIQUE (event_id, charge_type)` index bounds each execution to an index
// probe of ≤2 rows — so it is index-bounded, NOT the unbounded per-fleet scan
// the fleet-list aggregates were (those now read the trigger-maintained
// `core.fleet_activity_counters` row; this query stays event-scoped, and a page
// here is one fleet, not the workspace-wide Live Wall).
const EVENTS_SELECT =
    \\SELECT fleet_id::text, event_id, workspace_id::text, actor, event_type,
    \\       status, request_json::text, response_text, tokens, wall_ms,
    \\       failure_label, checkpoint_id, resumes_event_id,
    \\       created_at, updated_at,
    \\       (SELECT SUM(te.credit_deducted_nanos)::bigint
    \\          FROM core.fleet_execution_telemetry te
    \\         WHERE te.event_id = core.fleet_events.event_id
    \\           AND te.fleet_id = core.fleet_events.fleet_id::text) AS cost_nanos
    \\FROM core.fleet_events
    \\
;

pub const EventRow = struct {
    const Self = @This();

    fleet_id: []u8,
    event_id: []u8,
    workspace_id: []u8,
    actor: []u8,
    event_type: []u8,
    status: []u8,
    request_json: []u8,
    response_text: ?[]u8,
    tokens: ?i64,
    wall_ms: ?i64,
    failure_label: ?[]u8,
    checkpoint_id: ?[]u8,
    resumes_event_id: ?[]u8,
    created_at: i64,
    updated_at: i64,
    /// Summed `credit_deducted_nanos` over this event's telemetry rows.
    /// `null` when the event recorded no telemetry — the ledger renders that
    /// as unknown, never as a zero charge. Scalar: nothing to free in deinit.
    cost_nanos: ?i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.fleet_id);
        alloc.free(self.event_id);
        alloc.free(self.workspace_id);
        alloc.free(self.actor);
        alloc.free(self.event_type);
        alloc.free(self.status);
        alloc.free(self.request_json);
        if (self.response_text) |v| alloc.free(v);
        if (self.failure_label) |v| alloc.free(v);
        if (self.checkpoint_id) |v| alloc.free(v);
        if (self.resumes_event_id) |v| alloc.free(v);
    }
};

// ── Per-fleet listing ──────────────────────────────────────────────────────

pub fn listForFleet(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    fleet_id: []const u8,
    filter: Filter,
) ![]EventRow {
    const lim: i32 = @intCast(filter.limit);

    if (filter.cursor) |c| {
        const parsed = try parseCursor(alloc, c);
        defer alloc.free(parsed.event_id);
        if (filter.actor_like) |al| {
            return queryRows(conn, alloc, EVENTS_SELECT ++
                \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
                \\  AND (created_at, event_id) < ($3, $4)
                \\  AND actor LIKE $5
                \\ORDER BY created_at DESC, event_id DESC
                \\LIMIT $6
            , .{ workspace_id, fleet_id, parsed.created_at, parsed.event_id, al, lim });
        }
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
            \\  AND (created_at, event_id) < ($3, $4)
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $5
        , .{ workspace_id, fleet_id, parsed.created_at, parsed.event_id, lim });
    }
    if (filter.since_ms) |since| {
        if (filter.actor_like) |al| {
            return queryRows(conn, alloc, EVENTS_SELECT ++
                \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
                \\  AND created_at >= $3 AND actor LIKE $4
                \\ORDER BY created_at DESC, event_id DESC
                \\LIMIT $5
            , .{ workspace_id, fleet_id, since, al, lim });
        }
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
            \\  AND created_at >= $3
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $4
        , .{ workspace_id, fleet_id, since, lim });
    }
    if (filter.actor_like) |al| {
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
            \\  AND actor LIKE $3
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $4
        , .{ workspace_id, fleet_id, al, lim });
    }
    return queryRows(conn, alloc, EVENTS_SELECT ++
        \\WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid
        \\ORDER BY created_at DESC, event_id DESC
        \\LIMIT $3
    , .{ workspace_id, fleet_id, lim });
}

// ── Workspace-aggregate listing ─────────────────────────────────────────────

pub fn listForWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    fleet_id_filter: ?[]const u8,
    filter: Filter,
) ![]EventRow {
    if (fleet_id_filter) |zid| {
        return listForFleet(conn, alloc, workspace_id, zid, filter);
    }

    const lim: i32 = @intCast(filter.limit);

    if (filter.cursor) |c| {
        const parsed = try parseCursor(alloc, c);
        defer alloc.free(parsed.event_id);
        if (filter.actor_like) |al| {
            return queryRows(conn, alloc, EVENTS_SELECT ++
                \\WHERE workspace_id = $1::uuid
                \\  AND (created_at, event_id) < ($2, $3)
                \\  AND actor LIKE $4
                \\ORDER BY created_at DESC, event_id DESC
                \\LIMIT $5
            , .{ workspace_id, parsed.created_at, parsed.event_id, al, lim });
        }
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid
            \\  AND (created_at, event_id) < ($2, $3)
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $4
        , .{ workspace_id, parsed.created_at, parsed.event_id, lim });
    }
    if (filter.since_ms) |since| {
        if (filter.actor_like) |al| {
            return queryRows(conn, alloc, EVENTS_SELECT ++
                \\WHERE workspace_id = $1::uuid AND created_at >= $2 AND actor LIKE $3
                \\ORDER BY created_at DESC, event_id DESC
                \\LIMIT $4
            , .{ workspace_id, since, al, lim });
        }
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid AND created_at >= $2
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $3
        , .{ workspace_id, since, lim });
    }
    if (filter.actor_like) |al| {
        return queryRows(conn, alloc, EVENTS_SELECT ++
            \\WHERE workspace_id = $1::uuid AND actor LIKE $2
            \\ORDER BY created_at DESC, event_id DESC
            \\LIMIT $3
        , .{ workspace_id, al, lim });
    }
    return queryRows(conn, alloc, EVENTS_SELECT ++
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC, event_id DESC
        \\LIMIT $2
    , .{ workspace_id, lim });
}

// ── Internals ──────────────────────────────────────────────────────────────

fn queryRows(conn: *pg.Conn, alloc: std.mem.Allocator, comptime sql: []const u8, params: anytype) ![]EventRow {
    var q = PgQuery.from(try conn.query(sql, params));
    defer q.deinit();

    var rows: std.ArrayList(EventRow) = .empty;
    errdefer {
        for (rows.items) |*r| r.deinit(alloc);
        rows.deinit(alloc);
    }

    while (try q.next()) |row| {
        var er = try readRow(alloc, row);
        errdefer er.deinit(alloc);
        try rows.append(alloc, er);
    }
    return rows.toOwnedSlice(alloc);
}

fn readRow(alloc: std.mem.Allocator, row: pg.Row) !EventRow {
    const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(fleet_id);
    const event_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(event_id);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(workspace_id);
    const actor = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(actor);
    const event_type = try alloc.dupe(u8, try row.get([]const u8, 4));
    errdefer alloc.free(event_type);
    const status = try alloc.dupe(u8, try row.get([]const u8, 5));
    errdefer alloc.free(status);
    const request_json = try alloc.dupe(u8, try row.get([]const u8, 6));
    errdefer alloc.free(request_json);
    const response_text = try dupeOptionalString(alloc, row, 7);
    errdefer if (response_text) |v| alloc.free(v);
    const tokens = try row.get(?i64, 8);
    const wall_ms = try row.get(?i64, 9);
    const failure_label = try dupeOptionalString(alloc, row, 10);
    errdefer if (failure_label) |v| alloc.free(v);
    const checkpoint_id = try dupeOptionalString(alloc, row, 11);
    errdefer if (checkpoint_id) |v| alloc.free(v);
    const resumes_event_id = try dupeOptionalString(alloc, row, 12);
    errdefer if (resumes_event_id) |v| alloc.free(v);
    const created_at = try row.get(i64, 13);
    const updated_at = try row.get(i64, 14);
    const cost_nanos = try row.get(?i64, 15);

    return .{
        .fleet_id = fleet_id,
        .event_id = event_id,
        .workspace_id = workspace_id,
        .actor = actor,
        .event_type = event_type,
        .status = status,
        .request_json = request_json,
        .response_text = response_text,
        .tokens = tokens,
        .wall_ms = wall_ms,
        .failure_label = failure_label,
        .checkpoint_id = checkpoint_id,
        .resumes_event_id = resumes_event_id,
        .created_at = created_at,
        .updated_at = updated_at,
        .cost_nanos = cost_nanos,
    };
}

fn dupeOptionalString(alloc: std.mem.Allocator, row: pg.Row, idx: usize) !?[]u8 {
    const val = try row.get(?[]const u8, idx);
    if (val) |v| return try alloc.dupe(u8, v);
    return null;
}

test {
    _ = @import("fleet_events_store_test.zig");
}
