const std = @import("std");

const db = @import("../db/pool.zig");
const common = @import("common.zig");

const log = std.log.scoped(.zombied);

pub fn run(alloc: std.mem.Allocator) !void {
    const pool = db.initFromEnvForRole(alloc, .api) catch |err| {
        std.debug.print("fatal: api database init failed: {}\n", .{err});
        std.process.exit(1);
    };
    defer pool.deinit();

    common.runCanonicalMigrations(pool) catch |err| {
        std.debug.print("fatal: schema migration failed: {}\n", .{err});
        std.process.exit(1);
    };
    log.info("schema migrations completed", .{});
}
