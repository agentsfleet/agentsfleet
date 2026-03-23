const std = @import("std");

const db = @import("../db/pool.zig");
const common = @import("common.zig");
const error_codes = @import("../errors/codes.zig");

const log = std.log.scoped(.zombied);

pub fn run(alloc: std.mem.Allocator) !void {
    log.info("migrate.start status=connecting role=migrator", .{});
    const pool = db.initFromEnvForRole(alloc, .migrator) catch |err| {
        log.err("migrate.db_connect status=fail error_code={s} role=migrator err={s}", .{ error_codes.ERR_STARTUP_DB_CONNECT, @errorName(err) });
        std.process.exit(1);
    };
    defer pool.deinit();

    log.info("migrate.start status=running", .{});
    common.runCanonicalMigrations(pool) catch |err| {
        log.err("migrate.run status=fail error_code={s} err={s}", .{ error_codes.ERR_STARTUP_MIGRATION_CHECK, @errorName(err) });
        std.process.exit(1);
    };
    log.info("migrate.ok status=completed", .{});
}
