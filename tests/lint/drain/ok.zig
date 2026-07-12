//! Lint fixture: `PgQuery.from` correctly paired with `defer q.deinit()`. Clean.
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

pub fn clean(conn: *Conn) !void {
    var q = PgQuery.from(try conn.query("SELECT 1"));
    defer q.deinit();
}
