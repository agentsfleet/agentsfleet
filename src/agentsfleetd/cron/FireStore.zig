//! Read-only schedule resolution for signed QStash fires.

const FireStore = @This();

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const fleet_config = @import("../fleet_runtime/config.zig");
const model = @import("model.zig");
const sql = @import("sql.zig");

pool: *pg.Pool,

pub const Target = struct {
    fleet_id: []u8,
    workspace_id: []u8,
    message: []u8,
    generation: i64,
    desired_status: model.DesiredStatus,
    sync_status: model.SyncStatus,
    fleet_status: fleet_config.FleetStatus,

    pub fn deinit(self: *Target, alloc: std.mem.Allocator) void {
        alloc.free(self.fleet_id);
        alloc.free(self.workspace_id);
        alloc.free(self.message);
        self.* = undefined;
    }

    pub fn isRunnable(self: Target, generation: i64) bool {
        return self.generation == generation and
            self.desired_status == .active and
            self.sync_status == .synced and
            self.fleet_status.isRunnable();
    }
};

pub fn init(pool: *pg.Pool) FireStore {
    return .{ .pool = pool };
}

pub fn resolve(self: FireStore, alloc: std.mem.Allocator, schedule_id: []const u8) !?Target {
    const conn = try self.pool.acquire();
    defer self.pool.release(conn);
    var query = PgQuery.from(try conn.query(sql.FIRE_TARGET, .{schedule_id}));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    const desired = model.DesiredStatus.fromSlice(try row.get([]const u8, 4)) orelse return error.InvalidScheduleRow;
    const sync = model.SyncStatus.fromSlice(try row.get([]const u8, 5)) orelse return error.InvalidScheduleRow;
    const fleet_status = fleet_config.FleetStatus.fromSlice(try row.get([]const u8, 6)) orelse return error.InvalidFleetRow;
    const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(fleet_id);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(workspace_id);
    const message = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(message);
    return .{
        .fleet_id = fleet_id,
        .workspace_id = workspace_id,
        .message = message,
        .generation = try row.get(i64, 3),
        .desired_status = desired,
        .sync_status = sync,
        .fleet_status = fleet_status,
    };
}
