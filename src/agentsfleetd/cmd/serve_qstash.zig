//! Boot-only QStash credential loading; request threads borrow the result.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const Credentials = @import("../cron/Credentials.zig");

const log = logging.scoped(.agentsfleetd);
const EVENT_QSTASH_UNCONFIGURED = "startup.qstash_unconfigured";

pub fn load(
    alloc: std.mem.Allocator,
    pool: *pg.Pool,
    admin_workspace_id: []const u8,
) ?Credentials {
    const conn = pool.acquire() catch |err| {
        log.warn(EVENT_QSTASH_UNCONFIGURED, .{ .reason = @errorName(err) });
        return null;
    };
    defer pool.release(conn);
    return Credentials.load(alloc, conn, admin_workspace_id) catch |err| {
        log.warn(EVENT_QSTASH_UNCONFIGURED, .{ .reason = @errorName(err) });
        return null;
    };
}
