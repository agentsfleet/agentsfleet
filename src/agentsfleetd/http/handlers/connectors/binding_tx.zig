//! Atomic persistence guard for one workspace/provider connector binding.

const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const ec = @import("../../../errors/error_registry.zig");
const sql = @import("sql.zig");

const BindingTxn = @This();
const log = logging.scoped(.connectors);

conn: *pg.Conn,
open: bool,

/// Begin a transaction and serialize writers for one connector binding.
pub fn begin(conn: *pg.Conn, provider: []const u8, workspace_id: []const u8) !BindingTxn {
    try conn.begin();
    var txn = BindingTxn{ .conn = conn, .open = true };
    errdefer txn.abort();

    var query = PgQuery.from(try conn.query(sql.LOCK_WORKSPACE_CONNECTOR, .{ provider, workspace_id }));
    defer query.deinit();
    _ = try query.next() orelse return error.LockFailed;
    return txn;
}

/// Commit every connector row written under this guard.
pub fn commit(self: *BindingTxn) !void {
    if (!self.open) return;
    try self.conn.commit();
    self.open = false;
}

/// Roll back an open transaction. Safe to call after commit or twice.
pub fn abort(self: *BindingTxn) void {
    if (!self.open) return;
    self.open = false;
    self.conn.rollback() catch |err| log.warn("connector_binding_rollback_failed", .{
        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
        .err = @errorName(err),
    });
}

test "abort is idempotent for a closed binding transaction" {
    var txn: BindingTxn = undefined;
    txn.open = false;
    txn.abort();
    txn.abort();
}
