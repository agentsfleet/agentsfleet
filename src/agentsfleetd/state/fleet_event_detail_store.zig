//! Detail read for ONE `core.fleet_events` row, bodies included.
//!
//! Sibling to `fleet_events_store.zig`, which lists. The split is the point:
//! the list renders a page of up to two hundred rows and pays for every column
//! it selects, while the body and the agent's full answer are wanted one row
//! at a time. Keeping them on separate statements is what lets the list stop
//! carrying them without the expanded view losing anything.
//!
//! The read is keyed on `(fleet_id, event_id)` — the table's primary key — with
//! `workspace_id` as an additional predicate rather than a separate check. A
//! row belonging to another workspace therefore returns no row, which the
//! handler renders identically to an unknown identifier: existence is never
//! disclosed across a workspace boundary.
//!
//! Caller-owned allocator: methods that allocate (incl. deinit) take the
//! allocator as a parameter.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// `cost_nanos` is summed over this event's telemetry rows the same way the list
// read sums it — billing writes up to two per event (`receive`, `stage`, unique
// on `(event_id, charge_type)`), so a bare LEFT JOIN would duplicate the row
// per leg. The correlated subselect keeps one row and yields SQL NULL when no
// telemetry exists: an unbilled run reads as "unknown", never as zero.
//
// Bounded by construction — the WHERE clause names the full primary key, so
// this executes for exactly one event no matter how much history the fleet has.
const DETAIL_SELECT =
    \\SELECT fleet_id::text, event_id, workspace_id::text, actor, event_type,
    \\       status, request_json::text, response_text, tokens, wall_ms,
    \\       failure_label, failure_detail, checkpoint_id, resumes_event_id,
    \\       created_at, updated_at,
    \\       (SELECT SUM(te.credit_deducted_nanos)::bigint
    \\          FROM billing.usage_ledger te
    \\         WHERE te.event_id = core.fleet_events.event_id
    \\           AND te.fleet_id = core.fleet_events.fleet_id) AS cost_nanos
    \\FROM core.fleet_events
    \\WHERE fleet_id = $1::uuid AND event_id = $2 AND workspace_id = $3::uuid
;

/// One event with everything recorded about it. The two body columns —
/// `request_json` and `response_text` — are what distinguish this from the
/// list row; every other field is the same value the list already carried, so
/// an expanded row needs no second request to stay consistent with its table.
pub const EventDetailRow = struct {
    const Self = @This();

    fleet_id: []u8,
    event_id: []u8,
    workspace_id: []u8,
    actor: []u8,
    event_type: []u8,
    status: []u8,
    /// The trigger payload as stored, serialized to text.
    request_json: []u8,
    /// The agent's full answer. NULL while a run is in flight, and on a run
    /// that failed before producing one.
    response_text: ?[]u8,
    tokens: ?i64,
    wall_ms: ?i64,
    failure_label: ?[]u8,
    /// Human-readable cause line from the runner's classification site; NULL
    /// on success or when an older runner omitted it.
    failure_detail: ?[]u8,
    checkpoint_id: ?[]u8,
    resumes_event_id: ?[]u8,
    created_at: i64,
    updated_at: i64,
    /// Summed `credit_deducted_nanos` over this event's telemetry rows. `null`
    /// when the event recorded no telemetry — rendered as unknown, never as a
    /// zero charge. Scalar: nothing to free in deinit.
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
        if (self.failure_detail) |v| alloc.free(v);
        if (self.checkpoint_id) |v| alloc.free(v);
        if (self.resumes_event_id) |v| alloc.free(v);
    }
};

/// The one event, or `null` when this workspace's fleet holds no such row.
///
/// `null` deliberately conflates "no such event" with "an event that exists
/// elsewhere" — the workspace predicate is inside the statement, so this
/// function cannot tell the two apart and therefore cannot leak the difference.
pub fn getForFleet(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
) !?EventDetailRow {
    var q = PgQuery.from(try conn.query(DETAIL_SELECT, .{ fleet_id, event_id, workspace_id }));
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    return try readRow(alloc, row);
}

fn readRow(alloc: std.mem.Allocator, row: pg.Row) !EventDetailRow {
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
    const failure_detail = try dupeOptionalString(alloc, row, 11);
    errdefer if (failure_detail) |v| alloc.free(v);
    const checkpoint_id = try dupeOptionalString(alloc, row, 12);
    errdefer if (checkpoint_id) |v| alloc.free(v);
    const resumes_event_id = try dupeOptionalString(alloc, row, 13);
    errdefer if (resumes_event_id) |v| alloc.free(v);

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
        .failure_detail = failure_detail,
        .checkpoint_id = checkpoint_id,
        .resumes_event_id = resumes_event_id,
        .created_at = try row.get(i64, 14),
        .updated_at = try row.get(i64, 15),
        .cost_nanos = try row.get(?i64, 16),
    };
}

fn dupeOptionalString(alloc: std.mem.Allocator, row: pg.Row, idx: usize) !?[]u8 {
    const val = try row.get(?[]const u8, idx);
    if (val) |v| return try alloc.dupe(u8, v);
    return null;
}

test "DETAIL_SELECT keys on the primary key and scopes to the workspace" {
    // The statement's safety property is structural, not behavioural: the
    // workspace predicate must live INSIDE the SQL. A handler-side check after
    // the read would answer 403-vs-404 differently and leak existence.
    try std.testing.expect(std.mem.indexOf(u8, DETAIL_SELECT, "WHERE fleet_id = $1::uuid AND event_id = $2 AND workspace_id = $3::uuid") != null);
    try std.testing.expect(std.mem.indexOf(u8, DETAIL_SELECT, "core.fleet_events") != null);
    // Both bodies are here and nowhere else — that is this file's whole reason.
    try std.testing.expect(std.mem.indexOf(u8, DETAIL_SELECT, "request_json::text") != null);
    try std.testing.expect(std.mem.indexOf(u8, DETAIL_SELECT, "response_text") != null);
}
