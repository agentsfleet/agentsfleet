const std = @import("std");

const db = @import("../db/pool.zig");
const common = @import("common.zig");
const error_codes = @import("../errors/error_registry.zig");
const preflight = @import("preflight.zig");
const logging = @import("log");

const log = logging.scoped(.zombied);

const max_migrate_attempts = 3;
const retry_delay_ms: u64 = 2_000;

pub fn run(alloc: std.mem.Allocator) !void {
    preflight.initOtelLogs(alloc);
    defer preflight.deinitOtelLogs();

    var attempt: u32 = 1;
    while (true) {
        log.info("migrate.connect_start", .{ .role = "migrator", .attempt = attempt, .max_attempts = max_migrate_attempts });
        const pool = db.initFromEnvForRole(alloc, .migrator) catch |err| {
            if (attempt < max_migrate_attempts and isRetryable(err)) {
                log.warn("migrate.connect_retry", .{
                    .attempt = attempt,
                    .max_attempts = max_migrate_attempts,
                    .err = @errorName(err),
                    .delay_ms = retry_delay_ms,
                });
                std.Thread.sleep(retry_delay_ms * std.time.ns_per_ms);
                attempt += 1;
                continue;
            }
            log.err("migrate.db_connect_failed", .{
                .error_code = error_codes.ERR_STARTUP_DB_CONNECT,
                .role = "migrator",
                .err = @errorName(err),
            });
            preflight.deinitOtelLogs();
            std.process.exit(1);
        };

        log.info("migrate.run_start", .{ .attempt = attempt, .max_attempts = max_migrate_attempts });
        common.runCanonicalMigrations(pool) catch |err| {
            pool.deinit();
            if (attempt < max_migrate_attempts and isRetryable(err)) {
                log.warn("migrate.run_retry", .{
                    .attempt = attempt,
                    .max_attempts = max_migrate_attempts,
                    .err = @errorName(err),
                    .delay_ms = retry_delay_ms,
                });
                std.Thread.sleep(retry_delay_ms * std.time.ns_per_ms);
                attempt += 1;
                continue;
            }
            log.err("migrate.run_failed", .{
                .error_code = error_codes.ERR_STARTUP_MIGRATION_CHECK,
                .err = @errorName(err),
            });
            preflight.deinitOtelLogs();
            std.process.exit(1);
        };
        pool.deinit();
        break;
    }
    log.info("migrate.completed", .{ .attempt = attempt, .max_attempts = max_migrate_attempts });
}

fn isRetryable(err: anyerror) bool {
    return err == error.UnexpectedDBMessage or
        err == error.ConnectionRefused or
        err == error.ConnectionTimedOut or
        err == error.ConnectionResetByPeer or
        err == error.BrokenPipe;
}
