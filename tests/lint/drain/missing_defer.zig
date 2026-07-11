//! Lint fixture: a `PgQuery.from` binding with no `defer q.deinit()`. The drain
//! check must flag it — the deinit that auto-drains the result never fires, so
//! the pooled connection is returned with an open result.
const PgQuery = struct {
    pub fn from(x: anytype) PgQuery {
        _ = x;
        return .{};
    }
    pub fn deinit(self: *PgQuery) void {
        _ = self;
    }
};

const Conn = struct {
    pub fn query(self: *Conn, sql: []const u8) !u8 {
        _ = self;
        _ = sql;
        return 0;
    }
};

pub fn leaky(conn: *Conn) !void {
    var q = PgQuery.from(try conn.query("SELECT 1"));
    _ = &q; // BUG under test: missing `defer q.deinit();`
}
