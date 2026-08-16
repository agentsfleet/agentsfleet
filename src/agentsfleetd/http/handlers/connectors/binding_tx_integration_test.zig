//! Real PostgreSQL proof for workspace/provider connector-writer serialization.

const std = @import("std");
const pg = @import("pg");
const common_lib = @import("common");
const common = @import("../common.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const BindingTxn = @import("binding_tx.zig");

const testing = std.testing;
const PROVIDER = "binding-lock-test";
const WORKSPACE_ID = "0195c136-0002-7000-8000-000000000002";
const LOCK_POLL_LIMIT: usize = 250;
const LOCK_POLL_NS: u64 = 20 * std.time.ns_per_ms;

const LockWorker = struct {
    conn: *pg.Conn,
    acquired: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    failure: ?anyerror = null,

    fn run(self: *LockWorker) void {
        var txn = BindingTxn.begin(self.conn, PROVIDER, WORKSPACE_ID) catch |err| {
            self.failure = err;
            return;
        };
        self.acquired.store(true, .release);
        txn.abort();
    }
};

test "integration: connector writers wait on the shared workspace provider lock" {
    const db_ctx = (try common.openHandlerTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const worker_conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(worker_conn);
    const observer_conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(observer_conn);
    const worker_pid = try backendPid(worker_conn);

    var blocker = try BindingTxn.begin(db_ctx.conn, PROVIDER, WORKSPACE_ID);
    var worker = LockWorker{ .conn = worker_conn };
    const thread = try std.Thread.spawn(.{}, LockWorker.run, .{&worker});
    var joined = false;
    defer if (!joined) {
        blocker.abort();
        thread.join();
    };

    try waitForAdvisoryLock(observer_conn, worker_pid);
    try testing.expect(!worker.acquired.load(.acquire));

    blocker.abort();
    thread.join();
    joined = true;
    if (worker.failure) |err| return err;
    try testing.expect(worker.acquired.load(.acquire));
}

fn backendPid(conn: *pg.Conn) !i32 {
    var query = PgQuery.from(try conn.query("SELECT pg_backend_pid()", .{}));
    defer query.deinit();
    const row = try query.next() orelse return error.BackendPidMissing;
    return row.get(i32, 0);
}

fn waitForAdvisoryLock(conn: *pg.Conn, worker_pid: i32) !void {
    for (0..LOCK_POLL_LIMIT) |_| {
        var query = PgQuery.from(try conn.query(
            \\SELECT count(*)::bigint
            \\FROM pg_stat_activity
            \\WHERE pid = $1 AND wait_event_type = 'Lock' AND wait_event = 'advisory'
        , .{worker_pid}));
        defer query.deinit();
        const row = try query.next() orelse return error.LockWaitQueryEmpty;
        if (try row.get(i64, 0) == 1) return;
        query.drain();
        common_lib.sleepNanos(LOCK_POLL_NS);
    }
    return error.AdvisoryLockWaitNotObserved;
}
