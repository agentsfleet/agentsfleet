//! Append/read helpers for `fleet.runner_events`.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const protocol = @import("contract").protocol;

const log = logging.scoped(.fleet_runner_events);

pub const META_HOST_ID = "host_id";
pub const META_SANDBOX_TIER = "sandbox_tier";
pub const META_LEASE_ID = "lease_id";
pub const META_FLEET_ID = "fleet_id";
pub const META_AGENTSFLEET_EVENT_ID = "event_id";
pub const META_KIND = "kind";
pub const META_FROM_ADMIN_STATE = "from_admin_state";
pub const META_TO_ADMIN_STATE = "to_admin_state";
pub const META_LAST_SEEN_AT = "last_seen_at";

pub const Filter = struct {
    event_type: ?protocol.RunnerEventType = null,
    since: ?i64 = null,
    until: ?i64 = null,
};

const RunnerEventPage = struct {
    items: []protocol.RunnerEventItem,
    total: i64,
};

pub fn eventTypeForAdminState(state: protocol.AdminState) protocol.RunnerEventType {
    return switch (state) {
        .active => .runner_online,
        .cordoned => .runner_cordoned,
        .draining => .runner_draining,
        .drained => .runner_drained,
        .revoked => .runner_revoked,
    };
}

pub fn listForRunner(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    filter: Filter,
    page: i32,
    page_size: i32,
) !RunnerEventPage {
    const offset: i64 = @as(i64, page - 1) * @as(i64, page_size);
    const limit: i64 = page_size;
    const event_type = eventTypeName(filter);
    var q = PgQuery.from(try conn.query(
        \\WITH filtered AS (
        \\  SELECT id::text, runner_id::text, event_type, occurred_at, metadata::text
        \\  FROM fleet.runner_events
        \\  WHERE runner_id = $1::uuid
        \\    AND ($2::text IS NULL OR event_type = $2::text)
        \\    AND ($3::bigint IS NULL OR occurred_at >= $3::bigint)
        \\    AND ($4::bigint IS NULL OR occurred_at <= $4::bigint)
        \\),
        \\page AS (
        \\  SELECT id, runner_id, event_type, occurred_at, metadata,
        \\    COUNT(*) OVER()::bigint AS total,
        \\    false AS count_only,
        \\    ROW_NUMBER() OVER (ORDER BY occurred_at DESC, id DESC)::bigint AS page_ord
        \\  FROM filtered
        \\  ORDER BY occurred_at DESC, id DESC
        \\  LIMIT $5::bigint OFFSET $6::bigint
        \\),
        \\total_row AS (
        \\  SELECT NULL::text AS id, NULL::text AS runner_id, NULL::text AS event_type,
        \\    0::bigint AS occurred_at, NULL::text AS metadata,
        \\    (SELECT COUNT(*)::bigint FROM filtered) AS total,
        \\    true AS count_only,
        \\    NULL::bigint AS page_ord
        \\  WHERE NOT EXISTS (SELECT 1 FROM page)
        \\)
        \\SELECT * FROM page
        \\UNION ALL
        \\SELECT * FROM total_row
        \\ORDER BY count_only ASC, page_ord ASC NULLS LAST
    , .{ runner_id, event_type, filter.since, filter.until, limit, offset }));
    defer q.deinit();

    var items: std.ArrayList(protocol.RunnerEventItem) = .empty;
    errdefer items.deinit(alloc);
    var total: i64 = 0;
    while (try q.next()) |row| {
        const row_total = try row.get(i64, 5);
        if (try row.get(bool, 6)) {
            total = row_total;
            continue;
        }
        if (total == 0) total = row_total;
        try items.append(alloc, try readItem(alloc, row));
    }
    return .{ .items = try items.toOwnedSlice(alloc), .total = total };
}

fn eventTypeName(filter: Filter) ?[]const u8 {
    return if (filter.event_type) |event_type| @tagName(event_type) else null;
}

pub fn appendLeaseReleasedBestEffort(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    lease_id: []const u8,
    fleet_id: []const u8,
    fleet_event_id: []const u8,
) void {
    appendLeaseReleased(pool, alloc, runner_id, lease_id, fleet_id, fleet_event_id) catch |err| {
        log.warn("lease_released_event_failed", .{ .runner_id = runner_id, .lease_id = lease_id, .err = @errorName(err) });
    };
}

fn appendLeaseReleased(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    lease_id: []const u8,
    fleet_id: []const u8,
    fleet_event_id: []const u8,
) !void {
    const event_row_id = try id_format.generateRunnerEventId(alloc);
    defer alloc.free(event_row_id);
    const conn = try pool.acquire();
    defer pool.release(conn);
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_events
        \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::text, $4::bigint,
        \\        jsonb_build_object($5::text, $6::text, $7::text, $8::text, $9::text, $10::text),
        \\        NULL, $4::bigint)
    , .{
        event_row_id,
        runner_id,
        @tagName(protocol.RunnerEventType.lease_released),
        now_ms,
        META_LEASE_ID,
        lease_id,
        META_FLEET_ID,
        fleet_id,
        META_AGENTSFLEET_EVENT_ID,
        fleet_event_id,
    });
}

fn readItem(alloc: std.mem.Allocator, row: pg.Row) !protocol.RunnerEventItem {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(id);
    const runner_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(runner_id);
    const raw_type = try row.get([]const u8, 2);
    const event_type = std.meta.stringToEnum(protocol.RunnerEventType, raw_type) orelse return error.DbRowShape;
    const metadata_text = try row.get([]const u8, 4);
    const metadata = try std.json.parseFromSliceLeaky(std.json.Value, alloc, metadata_text, .{ .allocate = .alloc_always });
    return .{
        .id = id,
        .runner_id = runner_id,
        .event_type = event_type,
        .occurred_at = try row.get(i64, 3),
        .metadata = metadata,
    };
}
