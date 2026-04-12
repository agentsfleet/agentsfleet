//! M18_001: Per-delivery execution telemetry store.
//!
//! Writes to `zombie_execution_telemetry`. All queries use PgQuery (RULE FLS).
//! Idempotent: insertTelemetry uses ON CONFLICT DO NOTHING on event_id.
//! Cursor format: "<recorded_at_dec>:<id>" (same as activity_stream.zig).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.zombie_telemetry_store);

pub const TelemetryRow = struct {
    id: []u8,
    zombie_id: []u8,
    workspace_id: []u8,
    event_id: []u8,
    token_count: u64,
    time_to_first_token_ms: u64,
    epoch_wall_time_ms: i64,
    wall_seconds: u64,
    plan_tier: []u8,
    credit_deducted_cents: i64,
    recorded_at: i64,

    // bvisor pattern: comptime size assertion.
    comptime {
        std.debug.assert(@sizeOf(TelemetryRow) == 128);
    }

    pub fn deinit(self: *TelemetryRow, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.zombie_id);
        alloc.free(self.workspace_id);
        alloc.free(self.event_id);
        alloc.free(self.plan_tier);
    }
};

pub const InsertTelemetryParams = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    event_id: []const u8,
    token_count: u64,
    time_to_first_token_ms: u64,
    epoch_wall_time_ms: i64,
    wall_seconds: u64,
    plan_tier: []const u8,
    credit_deducted_cents: i64,
    recorded_at: i64,
};

/// Insert one telemetry row. ON CONFLICT DO NOTHING — safe to call on replay.
pub fn insertTelemetry(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: InsertTelemetryParams,
) !void {
    const row_id = try id_format.generateZombieId(alloc);
    defer alloc.free(row_id);

    _ = try conn.exec(
        \\INSERT INTO zombie_execution_telemetry
        \\  (id, zombie_id, workspace_id, event_id,
        \\   token_count, time_to_first_token_ms, epoch_wall_time_ms,
        \\   wall_seconds, plan_tier, credit_deducted_cents, recorded_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        \\ON CONFLICT (event_id) DO NOTHING
    , .{
        row_id,
        params.zombie_id,
        params.workspace_id,
        params.event_id,
        @as(i64, @intCast(params.token_count)),
        @as(i64, @intCast(params.time_to_first_token_ms)),
        params.epoch_wall_time_ms,
        @as(i64, @intCast(params.wall_seconds)),
        params.plan_tier,
        params.credit_deducted_cents,
        params.recorded_at,
    });
}

/// Customer query — returns newest-first rows for one zombie in one workspace.
/// cursor is "<recorded_at>:<id>" or null for first page.
pub fn listTelemetryForZombie(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    zombie_id: []const u8,
    limit: u32,
    cursor: ?[]const u8,
) ![]TelemetryRow {
    if (cursor) |c| {
        const parsed = try parseCursor(c);
        return queryRows(conn, alloc,
            \\SELECT id, zombie_id, workspace_id, event_id,
            \\       token_count, time_to_first_token_ms, epoch_wall_time_ms,
            \\       wall_seconds, plan_tier, credit_deducted_cents, recorded_at
            \\FROM zombie_execution_telemetry
            \\WHERE workspace_id = $1 AND zombie_id = $2
            \\  AND (recorded_at, id) < ($3, $4)
            \\ORDER BY recorded_at DESC, id DESC
            \\LIMIT $5
        , .{ workspace_id, zombie_id, parsed.recorded_at, parsed.id, @as(i64, @intCast(limit)) });
    }
    return queryRows(conn, alloc,
        \\SELECT id, zombie_id, workspace_id, event_id,
        \\       token_count, time_to_first_token_ms, epoch_wall_time_ms,
        \\       wall_seconds, plan_tier, credit_deducted_cents, recorded_at
        \\FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1 AND zombie_id = $2
        \\ORDER BY recorded_at DESC, id DESC
        \\LIMIT $3
    , .{ workspace_id, zombie_id, @as(i64, @intCast(limit)) });
}

/// Operator query — cross-workspace. All params optional. Newest-first. No cursor.
pub fn listTelemetryAll(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: ?[]const u8,
    zombie_id: ?[]const u8,
    after_ms: ?i64,
    limit: u32,
) ![]TelemetryRow {
    // Build query dynamically based on which filters are set.
    // Postgres: use NULL coalesce so unused params are no-ops.
    return queryRows(conn, alloc,
        \\SELECT id, zombie_id, workspace_id, event_id,
        \\       token_count, time_to_first_token_ms, epoch_wall_time_ms,
        \\       wall_seconds, plan_tier, credit_deducted_cents, recorded_at
        \\FROM zombie_execution_telemetry
        \\WHERE ($1::TEXT IS NULL OR workspace_id = $1)
        \\  AND ($2::TEXT IS NULL OR zombie_id = $2)
        \\  AND ($3::BIGINT IS NULL OR recorded_at > $3)
        \\ORDER BY recorded_at DESC, id DESC
        \\LIMIT $4
    , .{ workspace_id, zombie_id, after_ms, @as(i64, @intCast(limit)) });
}

/// Build a cursor string from the last row of a page.
pub fn makeCursor(alloc: std.mem.Allocator, row: TelemetryRow) ![]u8 {
    return std.fmt.allocPrint(alloc, "{d}:{s}", .{ row.recorded_at, row.id });
}

// ── Internal helpers ────────────────────────────────────────────────

const ParsedCursor = struct { recorded_at: i64, id: []const u8 };

// Max cursor id segment: UUID-format IDs are ≤36 chars; 128 is a generous cap.
const CURSOR_ID_MAX_LEN: usize = 128;

fn parseCursor(cursor: []const u8) !ParsedCursor {
    const sep = std.mem.indexOf(u8, cursor, ":") orelse return error.InvalidCursor;
    const ts = std.fmt.parseInt(i64, cursor[0..sep], 10) catch return error.InvalidCursor;
    const id = cursor[sep + 1 ..];
    if (id.len == 0 or id.len > CURSOR_ID_MAX_LEN) return error.InvalidCursor;
    return .{ .recorded_at = ts, .id = id };
}

fn queryRows(conn: *pg.Conn, alloc: std.mem.Allocator, comptime sql: []const u8, params: anytype) ![]TelemetryRow {
    var q = PgQuery.from(try conn.query(sql, params));
    defer q.deinit();

    var rows: std.ArrayList(TelemetryRow) = .{};
    errdefer {
        for (rows.items) |*r| r.deinit(alloc);
        rows.deinit(alloc);
    }

    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const zombie_id_s = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(zombie_id_s);
        const workspace_id_s = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(workspace_id_s);
        const event_id_s = try alloc.dupe(u8, try row.get([]const u8, 3));
        errdefer alloc.free(event_id_s);
        const plan_tier_s = try alloc.dupe(u8, try row.get([]const u8, 8));
        errdefer alloc.free(plan_tier_s);

        try rows.append(alloc, .{
            .id = id,
            .zombie_id = zombie_id_s,
            .workspace_id = workspace_id_s,
            .event_id = event_id_s,
            .token_count = @intCast(try row.get(i64, 4)),
            .time_to_first_token_ms = @intCast(try row.get(i64, 5)),
            .epoch_wall_time_ms = try row.get(i64, 6),
            .wall_seconds = @intCast(try row.get(i64, 7)),
            .plan_tier = plan_tier_s,
            .credit_deducted_cents = try row.get(i64, 9),
            .recorded_at = try row.get(i64, 10),
        });
    }

    return rows.toOwnedSlice(alloc);
}
