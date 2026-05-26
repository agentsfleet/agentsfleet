// Approval gate inbox reads — list and single-row queries for the dashboard.
//
// Sibling of approval_gate_db.zig (writes); split to honor the file-length
// budget. Both files share the schema-level append-only invariant; reads are
// snapshot-consistent within a single statement, no cross-row locking.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const keyset_cursor = @import("keyset_cursor.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const GateStatus = @import("approval_gate.zig").GateStatus;
const PENDING_STATUS = GateStatus.pending.toSlice();

pub const PendingRow = struct {
    gate_id: []const u8,
    zombie_id: []const u8,
    zombie_name: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
    gate_kind: []const u8,
    proposed_action: []const u8,
    evidence_json: []const u8,
    blast_radius: []const u8,
    status: []const u8,
    detail: []const u8,
    requested_at: i64,
    timeout_at: i64,
    updated_at: ?i64,
    resolved_by: []const u8,

    pub fn deinit(self: *PendingRow, alloc: Allocator) void {
        alloc.free(self.gate_id);
        alloc.free(self.zombie_id);
        alloc.free(self.zombie_name);
        alloc.free(self.workspace_id);
        alloc.free(self.action_id);
        alloc.free(self.tool_name);
        alloc.free(self.action_name);
        alloc.free(self.gate_kind);
        alloc.free(self.proposed_action);
        alloc.free(self.evidence_json);
        alloc.free(self.blast_radius);
        alloc.free(self.status);
        alloc.free(self.detail);
        alloc.free(self.resolved_by);
    }
};

pub const ListFilter = struct {
    workspace_id: []const u8,
    status: ?[]const u8 = null,
    zombie_id: ?[]const u8 = null,
    gate_kind: ?[]const u8 = null,
};

pub const ListResult = struct {
    items: []PendingRow,

    pub fn deinit(self: *ListResult, alloc: Allocator) void {
        for (self.items) |*r| r.deinit(alloc);
        alloc.free(self.items);
    }
};

pub fn listPending(
    pool: *pg.Pool,
    alloc: Allocator,
    filter: ListFilter,
    cursor: ?keyset_cursor.Cursor,
    limit: u32,
) !ListResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const status_param: []const u8 = filter.status orelse PENDING_STATUS;
    const zombie_param: []const u8 = filter.zombie_id orelse "";
    const kind_param: []const u8 = filter.gate_kind orelse "";
    const cursor_ts: i64 = if (cursor) |c| c.created_at_ms else 0;
    const cursor_id: []const u8 = if (cursor) |c| c.id else "";
    const has_cursor: bool = cursor != null;

    var q = PgQuery.from(try conn.query(
        \\SELECT g.id::text, g.zombie_id::text, COALESCE(z.name, ''),
        \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
        \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
        \\       g.status, g.detail, g.requested_at, g.timeout_at,
        \\       g.updated_at, g.resolved_by
        \\FROM core.zombie_approval_gates g
        \\JOIN core.zombies z ON z.id = g.zombie_id
        \\WHERE g.workspace_id = $1::uuid
        \\  AND g.status = $2
        \\  AND ($3 = '' OR g.zombie_id = $3::uuid)
        \\  AND ($4 = '' OR g.gate_kind = $4)
        \\  AND ($5 = false OR (g.requested_at, g.id::text) > ($6, $7))
        \\ORDER BY g.requested_at ASC, g.id ASC
        \\LIMIT $8
    , .{
        filter.workspace_id, status_param, zombie_param, kind_param,
        has_cursor, cursor_ts, cursor_id, @as(i64, @intCast(limit)),
    }));
    defer q.deinit();

    var items: std.ArrayList(PendingRow) = .{};
    errdefer {
        for (items.items) |*r| r.deinit(alloc);
        items.deinit(alloc);
    }

    while (try q.next()) |row| {
        try items.append(alloc, try readPendingRow(alloc, row));
    }

    return .{ .items = try items.toOwnedSlice(alloc) };
}

/// Single-row read by row id, scoped to a workspace. Returns null when the
/// row doesn't exist OR belongs to a different workspace (cross-tenant lookup
/// leaks no information beyond "not found").
pub fn getByGateId(
    pool: *pg.Pool,
    alloc: Allocator,
    gate_id: []const u8,
    workspace_id: []const u8,
) !?PendingRow {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT g.id::text, g.zombie_id::text, COALESCE(z.name, ''),
        \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
        \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
        \\       g.status, g.detail, g.requested_at, g.timeout_at,
        \\       g.updated_at, g.resolved_by
        \\FROM core.zombie_approval_gates g
        \\JOIN core.zombies z ON z.id = g.zombie_id
        \\WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid
    , .{ gate_id, workspace_id }));
    defer q.deinit();

    if (try q.next()) |row| {
        return try readPendingRow(alloc, row);
    }
    return null;
}

fn readPendingRow(alloc: Allocator, row: pg.Row) !PendingRow {
    var owned: std.ArrayList([]const u8) = .{};
    errdefer {
        for (owned.items) |s| alloc.free(s);
        owned.deinit(alloc);
    }

    const gate_id = try dupTracked(alloc, &owned, try row.get([]const u8, 0));
    const zombie_id = try dupTracked(alloc, &owned, try row.get([]const u8, 1));
    const zombie_name = try dupTracked(alloc, &owned, try row.get([]const u8, 2));
    const workspace_id = try dupTracked(alloc, &owned, try row.get([]const u8, 3));
    const action_id = try dupTracked(alloc, &owned, try row.get([]const u8, 4));
    const tool_name = try dupTracked(alloc, &owned, try row.get([]const u8, 5));
    const action_name = try dupTracked(alloc, &owned, try row.get([]const u8, 6));
    const gate_kind = try dupTracked(alloc, &owned, try row.get([]const u8, 7));
    const proposed_action = try dupTracked(alloc, &owned, try row.get([]const u8, 8));
    const evidence_json = try dupTracked(alloc, &owned, try row.get([]const u8, 9));
    const blast_radius = try dupTracked(alloc, &owned, try row.get([]const u8, 10));
    const status = try dupTracked(alloc, &owned, try row.get([]const u8, 11));
    const detail = try dupTracked(alloc, &owned, try row.get([]const u8, 12));
    const requested_at = try row.get(i64, 13);
    const timeout_at = try row.get(i64, 14);
    const updated_at = try row.get(?i64, 15);
    const resolved_by = try dupTracked(alloc, &owned, try row.get([]const u8, 16));

    owned.deinit(alloc);
    return .{
        .gate_id = gate_id, .zombie_id = zombie_id, .zombie_name = zombie_name,
        .workspace_id = workspace_id, .action_id = action_id,
        .tool_name = tool_name, .action_name = action_name,
        .gate_kind = gate_kind, .proposed_action = proposed_action,
        .evidence_json = evidence_json, .blast_radius = blast_radius,
        .status = status, .detail = detail,
        .requested_at = requested_at, .timeout_at = timeout_at,
        .updated_at = updated_at, .resolved_by = resolved_by,
    };
}

fn dupTracked(alloc: Allocator, tracker: *std.ArrayList([]const u8), src: []const u8) ![]const u8 {
    const copy = try alloc.dupe(u8, src);
    errdefer alloc.free(copy);
    try tracker.append(alloc, copy);
    return copy;
}
