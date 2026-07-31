//! Append/read helpers for `fleet.runner_events`.

const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const protocol = @import("contract").protocol;

const log = logging.scoped(.fleet_runner_events);

pub const META_HOST_ID = "host_id";
pub const META_SANDBOX_TIER = "sandbox_tier";
pub const META_NETWORK_POLICY = "network_policy";
pub const META_REGISTRY_ALLOWLIST = "registry_allowlist";
pub const META_WORKER_COUNT = "worker_count";
pub const META_LEASE_ID = "lease_id";
pub const META_FLEET_ID = "fleet_id";
pub const META_AGENTSFLEET_EVENT_ID = "event_id";
pub const META_KIND = "kind";
pub const META_FROM_ADMIN_STATE = "from_admin_state";
pub const META_TO_ADMIN_STATE = "to_admin_state";
pub const META_LAST_SEEN_AT = "last_seen_at";

/// `event_types` is a set filter: empty means unfiltered, one value is the
/// old single-tag behaviour, several return the union. The handler validates
/// every tag before this layer sees it.
pub const Filter = struct {
    event_types: []const protocol.RunnerEventType = &.{},
    since: ?i64 = null,
    until: ?i64 = null,
};

const RunnerEventPage = struct {
    items: []protocol.RunnerEventItem,
    total: i64,
};

/// Keyset boundary for the events read — the previous page's last row.
pub const EventCursor = struct {
    occurred_at: i64,
    id: []const u8,
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
    cursor: ?EventCursor,
    limit: i64,
) !RunnerEventPage {
    const event_types = try eventTypeNames(alloc, filter);
    defer if (event_types) |names| alloc.free(names);

    const total = blk: {
        var count_q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_EVENT_COUNT, .{ runner_id, event_types, filter.since, filter.until }));
        defer count_q.deinit();
        const row = (try count_q.next()) orelse break :blk 0;
        break :blk try row.get(i64, 0);
    };

    var q = if (cursor) |c|
        PgQuery.from(try conn.query(sql.SELECT_RUNNER_EVENT_KEYSET_AFTER, .{ runner_id, event_types, filter.since, filter.until, c.occurred_at, c.id, limit }))
    else
        PgQuery.from(try conn.query(sql.SELECT_RUNNER_EVENT_KEYSET_FIRST, .{ runner_id, event_types, filter.since, filter.until, limit }));
    defer q.deinit();

    var items: std.ArrayList(protocol.RunnerEventItem) = .empty;
    errdefer items.deinit(alloc);
    while (try q.next()) |row| {
        try items.append(alloc, try readItem(alloc, row));
    }
    return .{ .items = try items.toOwnedSlice(alloc), .total = total };
}

/// Tag names for the SQL `text[]` bind; null means unfiltered. The tag-name
/// slices are static, only the outer slice is allocated — caller must free.
fn eventTypeNames(alloc: std.mem.Allocator, filter: Filter) !?[]const []const u8 {
    if (filter.event_types.len == 0) return null;
    const names = try alloc.alloc([]const u8, filter.event_types.len);
    for (filter.event_types, 0..) |event_type, i| names[i] = @tagName(event_type);
    return names;
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
        log.warn("lease_released_event_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .lease_id = lease_id, .err = @errorName(err) });
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
    _ = try conn.exec(sql.INSERT_RUNNER_EVENT, .{
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
