const std = @import("std");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const Store = @import("Store.zig");
const model = @import("model.zig");

pub const Fixture = struct {
    pool: *pg.Pool,
    store: Store,
    workspace_id: []const u8,
    fleet_id: []const u8,

    pub fn open(workspace_id: []const u8, fleet_id: []const u8) !?Fixture {
        const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return null;
        errdefer db_ctx.pool.deinit();
        errdefer db_ctx.pool.release(db_ctx.conn);
        try base.seedTenant(db_ctx.conn);
        try base.seedWorkspace(db_ctx.conn, workspace_id);
        try base.seedFleet(db_ctx.conn, fleet_id, workspace_id, "cron-store-test", "{}", "");
        db_ctx.pool.release(db_ctx.conn);
        return .{
            .pool = db_ctx.pool,
            .store = Store.init(db_ctx.pool),
            .workspace_id = workspace_id,
            .fleet_id = fleet_id,
        };
    }

    pub fn deinit(self: *Fixture) void {
        const conn = self.pool.acquire() catch {
            self.pool.deinit();
            return;
        };
        base.teardownFleets(conn, self.workspace_id);
        base.teardownWorkspace(conn, self.workspace_id);
        base.teardownTenant(conn);
        self.pool.release(conn);
        self.pool.deinit();
        self.* = undefined;
    }
};

pub fn createAndFinalize(
    fixture: *Fixture,
    alloc: std.mem.Allocator,
    schedule_id: []const u8,
    lease_token: []const u8,
) !model.Schedule {
    const input: model.CreateInput = .{
        .fleet_id = fixture.fleet_id,
        .source = .api,
        .source_key = schedule_id,
        .cron = "0 9 * * *",
        .timezone = "Asia/Kolkata",
        .message = "summarize today's Zoho Sprints",
    };
    var created = switch (try fixture.store.create(alloc, input, schedule_id, lease_token, 100, 200)) {
        .created => |schedule| schedule,
        else => return error.CreateFixtureFailed,
    };
    defer created.deinit(alloc);
    return (try fixture.store.finalizeSuccess(alloc, schedule_id, created.generation, lease_token, 101)) orelse
        error.FinalizeFixtureFailed;
}

pub fn indexedUuid(buf: *[36]u8, index: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, "0195b4ba-8d3a-7f13-8abc-{d:0>12}", .{index});
}
