const std = @import("std");

const db = @import("../db/pool.zig");
const obs_log = @import("../observability/logging.zig");
const common = @import("common.zig");

const log = std.log.scoped(.zombied);

pub fn run(alloc: std.mem.Allocator) !void {
    var args = std.process.args();
    _ = args.next();
    _ = args.next();
    const spec_path = args.next() orelse {
        std.debug.print("usage: zombied run <spec_path>\n", .{});
        std.process.exit(1);
    };

    log.info("one-shot run spec_path={s}", .{spec_path});

    const spec_content = std.fs.cwd().readFileAlloc(alloc, spec_path, 512 * 1024) catch |err| {
        std.debug.print("error reading spec: {}\n", .{err});
        std.process.exit(1);
    };
    defer alloc.free(spec_content);

    const pool = db.initFromEnvForRole(alloc, .worker) catch |err| {
        std.debug.print("fatal: database init failed: {}\n", .{err});
        std.process.exit(1);
    };
    defer pool.deinit();

    common.runCanonicalMigrations(pool) catch |err| {
        obs_log.logWarnErr(.zombied, err, "one-shot migration run skipped", .{});
    };

    log.info("spec loaded ({d} bytes) — pipeline runs via `zombied serve`", .{spec_content.len});
    log.info("one-shot mode: POST /v1/runs to trigger pipeline", .{});
}
