//! Postgres persistence for hosted schedules.
//!
//! The store retains only the pool pointer. Returned rows are allocated with
//! the allocator passed to each call and must be released with `Schedule.deinit`.

const Store = @This();

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const ec = @import("../errors/error_registry.zig");
const model = @import("model.zig");
const sql = @import("sql.zig");

const log = logging.scoped(.cron_store);

pool: *pg.Pool,

pub const CreateOutcome = union(enum) {
    created: model.Schedule,
    fleet_not_found,
    cap_reached,
    source_conflict,
};

pub const ClaimOutcome = union(enum) {
    claimed: model.Schedule,
    busy,
    not_found,
};

pub fn init(pool: *pg.Pool) Store {
    return .{ .pool = pool };
}

pub fn create(
    self: Store,
    alloc: std.mem.Allocator,
    input: model.CreateInput,
    schedule_id: []const u8,
    lease_token: []const u8,
    now_ms: i64,
    lease_until_ms: i64,
) !CreateOutcome {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);

    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    defer if (tx_open) rollbackLogged(conn);

    const count = try lockedScheduleCount(conn, input.fleet_id) orelse {
        try rollback(conn, &tx_open);
        return .{ .fleet_not_found = {} };
    };
    if (try sourceKeyExists(conn, input.fleet_id, input.source_key)) {
        try rollback(conn, &tx_open);
        return .{ .source_conflict = {} };
    }
    if (count >= model.MAX_SCHEDULES_PER_FLEET) {
        try rollback(conn, &tx_open);
        return .{ .cap_reached = {} };
    }

    var schedule = try insertRow(conn, alloc, input, schedule_id, lease_token, now_ms, lease_until_ms);
    errdefer schedule.deinit(alloc);
    _ = try conn.exec("COMMIT", .{});
    tx_open = false;
    return .{ .created = schedule };
}

pub fn get(self: Store, alloc: std.mem.Allocator, fleet_id: []const u8, schedule_id: []const u8) !?model.Schedule {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    var query = PgQuery.from(try conn.query(sql.SELECT_ONE, .{ schedule_id, fleet_id }));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    return try rowToSchedule(alloc, row);
}

pub fn list(self: Store, alloc: std.mem.Allocator, fleet_id: []const u8) ![]model.Schedule {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    var query = PgQuery.from(try conn.query(sql.LIST_FOR_FLEET, .{fleet_id}));
    defer query.deinit();

    var rows: std.ArrayList(model.Schedule) = .empty;
    errdefer {
        for (rows.items) |*row| row.deinit(alloc);
        rows.deinit(alloc);
    }
    while (try query.next()) |row| {
        var schedule = try rowToSchedule(alloc, row);
        errdefer schedule.deinit(alloc);
        try rows.append(alloc, schedule);
    }
    return rows.toOwnedSlice(alloc);
}

pub fn claimMutation(self: Store, alloc: std.mem.Allocator, input: model.MutationInput) !ClaimOutcome {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    {
        var query = PgQuery.from(try conn.query(sql.CLAIM_MUTATION, .{
            input.schedule_id,
            input.fleet_id,
            input.cron,
            input.timezone,
            input.message,
            input.desired_status.toSlice(),
            model.SyncStatus.syncing.toSlice(),
            input.lease_token,
            input.lease_until_ms,
            input.now_ms,
        }));
        defer query.deinit();
        if (try query.next()) |row| return .{ .claimed = try rowToSchedule(alloc, row) };
    }
    return if (try scheduleExists(conn, input.fleet_id, input.schedule_id))
        .{ .busy = {} }
    else
        .{ .not_found = {} };
}

pub fn finalizeSuccess(
    self: Store,
    alloc: std.mem.Allocator,
    schedule_id: []const u8,
    generation: i64,
    lease_token: []const u8,
    now_ms: i64,
) !?model.Schedule {
    return self.finalize(alloc, sql.FINALIZE_SUCCESS, .{ schedule_id, generation, lease_token, model.SyncStatus.synced.toSlice(), now_ms });
}

pub fn finalizeFailure(
    self: Store,
    alloc: std.mem.Allocator,
    schedule_id: []const u8,
    generation: i64,
    lease_token: []const u8,
    detail: []const u8,
    now_ms: i64,
) !?model.Schedule {
    return self.finalize(alloc, sql.FINALIZE_FAILURE, .{ schedule_id, generation, lease_token, model.SyncStatus.failed.toSlice(), detail, now_ms });
}

pub fn deleteClaimed(self: Store, schedule_id: []const u8, generation: i64, lease_token: []const u8) !bool {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    var query = PgQuery.from(try conn.query(sql.DELETE_CLAIMED, .{ schedule_id, generation, lease_token }));
    defer query.deinit();
    return (try query.next()) != null;
}

fn finalize(self: Store, alloc: std.mem.Allocator, statement: []const u8, args: anytype) !?model.Schedule {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    var query = PgQuery.from(try conn.query(statement, args));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    return try rowToSchedule(alloc, row);
}

fn lockedScheduleCount(conn: *pg.Conn, fleet_id: []const u8) !?usize {
    var query = PgQuery.from(try conn.query(sql.LOCK_FLEET_AND_COUNT, .{fleet_id}));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    return @intCast(try row.get(i64, 1));
}

fn sourceKeyExists(conn: *pg.Conn, fleet_id: []const u8, source_key: []const u8) !bool {
    var query = PgQuery.from(try conn.query(sql.SOURCE_KEY_EXISTS, .{ fleet_id, source_key }));
    defer query.deinit();
    return (try query.next()) != null;
}

fn scheduleExists(conn: *pg.Conn, fleet_id: []const u8, schedule_id: []const u8) !bool {
    var query = PgQuery.from(try conn.query(sql.EXISTS, .{ schedule_id, fleet_id }));
    defer query.deinit();
    return (try query.next()) != null;
}

fn insertRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    input: model.CreateInput,
    schedule_id: []const u8,
    lease_token: []const u8,
    now_ms: i64,
    lease_until_ms: i64,
) !model.Schedule {
    var query = PgQuery.from(try conn.query(sql.INSERT, .{
        schedule_id,
        input.fleet_id,
        input.source.toSlice(),
        input.source_key,
        input.cron,
        input.timezone,
        input.message,
        model.DesiredStatus.active.toSlice(),
        model.SyncStatus.syncing.toSlice(),
        @as(i64, 1),
        lease_token,
        lease_until_ms,
        now_ms,
    }));
    defer query.deinit();
    const row = (try query.next()) orelse return error.InsertReturnedNoRow;
    return rowToSchedule(alloc, row);
}

fn rowToSchedule(alloc: std.mem.Allocator, row: pg.Row) !model.Schedule {
    const source = model.Source.fromSlice(try row.get([]const u8, 2)) orelse return error.InvalidScheduleRow;
    const desired = model.DesiredStatus.fromSlice(try row.get([]const u8, 7)) orelse return error.InvalidScheduleRow;
    const sync = model.SyncStatus.fromSlice(try row.get([]const u8, 8)) orelse return error.InvalidScheduleRow;
    const schedule_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(schedule_id);
    const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(fleet_id);
    const source_key = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(source_key);
    const cron = try alloc.dupe(u8, try row.get([]const u8, 4));
    errdefer alloc.free(cron);
    const timezone = try alloc.dupe(u8, try row.get([]const u8, 5));
    errdefer alloc.free(timezone);
    const message = try alloc.dupe(u8, try row.get([]const u8, 6));
    errdefer alloc.free(message);
    const sync_token = try optionalDupe(alloc, try row.get(?[]const u8, 10));
    errdefer if (sync_token) |value| alloc.free(value);
    const last_error = try optionalDupe(alloc, try row.get(?[]const u8, 12));
    errdefer if (last_error) |value| alloc.free(value);
    return .{
        .schedule_id = schedule_id,
        .fleet_id = fleet_id,
        .source = source,
        .source_key = source_key,
        .cron = cron,
        .timezone = timezone,
        .message = message,
        .desired_status = desired,
        .sync_status = sync,
        .generation = try row.get(i64, 9),
        .sync_token = sync_token,
        .sync_lease_until = try row.get(?i64, 11),
        .last_error = last_error,
        .created_at = try row.get(i64, 13),
        .updated_at = try row.get(i64, 14),
    };
}

fn optionalDupe(alloc: std.mem.Allocator, value: ?[]const u8) !?[]const u8 {
    return if (value) |bytes| try alloc.dupe(u8, bytes) else null;
}

fn rollback(conn: *pg.Conn, tx_open: *bool) !void {
    try conn.rollback();
    tx_open.* = false;
}

fn rollbackLogged(conn: *pg.Conn) void {
    conn.rollback() catch |err| log.warn("schedule_store_rollback_failed", .{
        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
        .err = @errorName(err),
    });
}
