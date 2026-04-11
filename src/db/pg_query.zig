//! PgQuery — thin wrapper around pg.Result that auto-drains on deinit.
//!
//! Eliminates RULE PTR (q.*.next() anytype footgun) and the forgotten-drain
//! class of bugs by making cleanup unconditional via defer.
//!
//! Usage:
//!
//!   var q = PgQuery.from(try conn.query(sql, args));
//!   defer q.deinit();                  // auto-drains on any exit path
//!
//!   while (try q.next()) |row| { ... }
//!
//! Passing to helpers — use *PgQuery, never anytype:
//!
//!   fn helper(q: *PgQuery) !void {
//!       while (try q.next()) |row| { ... }
//!   }

const pg = @import("pg");

pub const PgQuery = struct {
    inner: *pg.Result,

    /// Wrap a *pg.Result returned by conn.query(). Use defer deinit() immediately.
    pub fn from(result: *pg.Result) PgQuery {
        return .{ .inner = result };
    }

    /// Returns the next row, or null when the result set is exhausted.
    pub fn next(self: *PgQuery) !?pg.Row {
        return self.inner.next();
    }

    /// Explicitly drain remaining rows. Idempotent — no-op if already idle.
    pub fn drain(self: *PgQuery) void {
        self.inner.drain() catch {};
    }

    /// Drain remaining rows then release the result. Always call via defer.
    pub fn deinit(self: *PgQuery) void {
        self.drain();
        self.inner.deinit();
    }
};

test "PgQuery: from + deinit API compiles" {
    const T = PgQuery;
    _ = T.from;
    _ = T.deinit;
    _ = T.next;
    _ = T.drain;
}

test {
    _ = @import("pg_query_test.zig");
}
