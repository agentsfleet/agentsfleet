//! M18_001: Per-delivery execution telemetry store.
//!
//! Writes to `zombie_execution_telemetry`. All queries use PgQuery (RULE FLS).
//! Idempotent: insertTelemetry uses ON CONFLICT DO NOTHING on event_id.
//! Cursor format: opaque base64url-no-pad token encoding "<recorded_at>:<id>".

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const base64 = std.base64.url_safe_no_pad;

const log = std.log.scoped(.zombie_telemetry_store);

// Shared SELECT columns reused across all query branches.
const TELEMETRY_SELECT =
    \\SELECT id, zombie_id, workspace_id, event_id,
    \\       token_count, time_to_first_token_ms, epoch_wall_time_ms,
    \\       wall_seconds, plan_tier, credit_deducted_cents, recorded_at
    \\FROM zombie_execution_telemetry
;

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

    // bvisor pattern: comptime size assertion — @compileError so it fires in all build modes.
    comptime {
        if (@sizeOf(TelemetryRow) != 128) @compileError("TelemetryRow size changed; update this assertion");
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

    const max_i64: u64 = @intCast(std.math.maxInt(i64));
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
        @as(i64, @intCast(@min(params.token_count, max_i64))),
        @as(i64, @intCast(@min(params.time_to_first_token_ms, max_i64))),
        params.epoch_wall_time_ms,
        @as(i64, @intCast(@min(params.wall_seconds, max_i64))),
        params.plan_tier,
        params.credit_deducted_cents,
        params.recorded_at,
    });
}

/// Customer query — returns newest-first rows for one zombie in one workspace.
/// cursor is an opaque base64url token (from makeCursor) or null for first page.
pub fn listTelemetryForZombie(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    zombie_id: []const u8,
    limit: u32,
    cursor: ?[]const u8,
) ![]TelemetryRow {
    if (cursor) |c| {
        const parsed = try parseCursor(alloc, c);
        return queryRows(conn, alloc, TELEMETRY_SELECT ++
            \\WHERE workspace_id = $1 AND zombie_id = $2
            \\  AND (recorded_at, id) < ($3, $4)
            \\ORDER BY recorded_at DESC, id DESC
            \\LIMIT $5
        , .{ workspace_id, zombie_id, parsed.recorded_at, parsed.id, @as(i64, @intCast(limit)) });
    }
    return queryRows(conn, alloc, TELEMETRY_SELECT ++
        \\WHERE workspace_id = $1 AND zombie_id = $2
        \\ORDER BY recorded_at DESC, id DESC
        \\LIMIT $3
    , .{ workspace_id, zombie_id, @as(i64, @intCast(limit)) });
}

/// Operator query — cross-workspace. All params optional. Newest-first. No cursor.
/// Uses static query branches (not IS NULL OR) so each active filter uses an index.
pub fn listTelemetryAll(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: ?[]const u8,
    zombie_id: ?[]const u8,
    after_ms: ?i64,
    limit: u32,
) ![]TelemetryRow {
    const lim: i64 = @intCast(limit);
    if (workspace_id) |ws| {
        if (zombie_id) |zid| {
            if (after_ms) |ams| {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE workspace_id = $1 AND zombie_id = $2 AND recorded_at > $3
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $4
                , .{ ws, zid, ams, lim });
            } else {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE workspace_id = $1 AND zombie_id = $2
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $3
                , .{ ws, zid, lim });
            }
        } else {
            if (after_ms) |ams| {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE workspace_id = $1 AND recorded_at > $2
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $3
                , .{ ws, ams, lim });
            } else {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE workspace_id = $1
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $2
                , .{ ws, lim });
            }
        }
    } else {
        if (zombie_id) |zid| {
            if (after_ms) |ams| {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE zombie_id = $1 AND recorded_at > $2
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $3
                , .{ zid, ams, lim });
            } else {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE zombie_id = $1
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $2
                , .{ zid, lim });
            }
        } else {
            if (after_ms) |ams| {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\WHERE recorded_at > $1
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $2
                , .{ ams, lim });
            } else {
                return queryRows(conn, alloc, TELEMETRY_SELECT ++
                    \\ORDER BY recorded_at DESC, id DESC
                    \\LIMIT $1
                , .{lim});
            }
        }
    }
}

/// Build an opaque base64url cursor token from the last row of a page.
pub fn makeCursor(alloc: std.mem.Allocator, row: TelemetryRow) ![]u8 {
    const plain = try std.fmt.allocPrint(alloc, "{d}:{s}", .{ row.recorded_at, row.id });
    defer alloc.free(plain);
    const encoded_len = base64.Encoder.calcSize(plain.len);
    const encoded = try alloc.alloc(u8, encoded_len);
    _ = base64.Encoder.encode(encoded, plain);
    return encoded;
}

// ── Internal helpers ────────────────────────────────────────────────

const ParsedCursor = struct { recorded_at: i64, id: []u8 };

// Max id segment: UUID-format IDs are ≤36 chars; 128 is a generous cap.
const CURSOR_ID_MAX_LEN: usize = 128;

/// Decode an opaque base64url cursor token into its components.
/// Caller owns ParsedCursor.id and must free it (or use an arena allocator).
fn parseCursor(alloc: std.mem.Allocator, cursor: []const u8) !ParsedCursor {
    // Base64url-decode the opaque token.
    const decoded_len = base64.Decoder.calcSizeForSlice(cursor) catch return error.InvalidCursor;
    const plain = try alloc.alloc(u8, decoded_len);
    defer alloc.free(plain);
    base64.Decoder.decode(plain, cursor) catch return error.InvalidCursor;

    const sep = std.mem.indexOf(u8, plain, ":") orelse return error.InvalidCursor;
    const ts = std.fmt.parseInt(i64, plain[0..sep], 10) catch return error.InvalidCursor;
    const id_slice = plain[sep + 1 ..];
    if (id_slice.len == 0 or id_slice.len > CURSOR_ID_MAX_LEN) return error.InvalidCursor;

    // Dupe id_slice before plain is freed by defer above.
    return .{ .recorded_at = ts, .id = try alloc.dupe(u8, id_slice) };
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
            // Safe cast: DB columns are BIGINT NOT NULL DEFAULT 0; @max(0,…) guards
            // against corrupt rows with negative values (impossible via insertTelemetry
            // but defensive against manual edits or future migrations).
            .token_count = @intCast(@max(0, try row.get(i64, 4))),
            .time_to_first_token_ms = @intCast(@max(0, try row.get(i64, 5))),
            .epoch_wall_time_ms = try row.get(i64, 6),
            .wall_seconds = @intCast(@max(0, try row.get(i64, 7))),
            .plan_tier = plan_tier_s,
            .credit_deducted_cents = try row.get(i64, 9),
            .recorded_at = try row.get(i64, 10),
        });
    }

    return rows.toOwnedSlice(alloc);
}

// ── Tests ───────────────────────────────────────────────────────────────────

// Comptime helper: base64url-encode a string literal for use in test inputs.
fn encode64(comptime src: []const u8) [base64.Encoder.calcSize(src.len)]u8 {
    var buf: [base64.Encoder.calcSize(src.len)]u8 = undefined;
    _ = base64.Encoder.encode(&buf, src);
    return buf;
}

test "parseCursor: valid round-trip" {
    const alloc = std.testing.allocator;
    const row = TelemetryRow{
        .id = @constCast("abc123"),
        .zombie_id = @constCast("z"),
        .workspace_id = @constCast("w"),
        .event_id = @constCast("e"),
        .token_count = 0,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 0,
        .wall_seconds = 0,
        .plan_tier = @constCast("free"),
        .credit_deducted_cents = 0,
        .recorded_at = 1712924400000,
    };
    const cursor = try makeCursor(alloc, row);
    defer alloc.free(cursor);
    const parsed = try parseCursor(alloc, cursor);
    defer alloc.free(parsed.id);
    try std.testing.expectEqual(@as(i64, 1712924400000), parsed.recorded_at);
    try std.testing.expectEqualStrings("abc123", parsed.id);
}

test "parseCursor: rejects invalid base64" {
    // Non-base64url characters must return InvalidCursor.
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, "!!not-valid-base64!!"));
}

test "parseCursor: rejects empty id" {
    const encoded = comptime encode64("1712924400000:");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects missing separator" {
    const encoded = comptime encode64("1712924400000");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects oversized id" {
    const big_id = "x" ** 129;
    const encoded = comptime encode64("1712924400000:" ++ big_id);
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects non-integer timestamp" {
    const encoded = comptime encode64("notanumber:abc123");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test {
    _ = @import("zombie_telemetry_store_test.zig");
}
