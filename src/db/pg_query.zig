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

const std = @import("std");
const pg = @import("pg");

pub const PgQuery = struct {
    inner: *pg.Result,

    // bvisor pattern: comptime size assertion — single pointer wrapper.
    comptime {
        std.debug.assert(@sizeOf(PgQuery) == 8);
    }

    /// Wrap a *pg.Result returned by conn.query(). Use defer deinit() immediately.
    pub fn from(result: *pg.Result) PgQuery {
        return .{ .inner = result };
    }

    /// Returns the next row, or null when the result set is exhausted.
    pub fn next(self: *PgQuery) !?pg.Row {
        return self.inner.next();
    }

    /// Explicitly drain remaining rows. Idempotent — no-op when there is
    /// nothing in flight on the connection.
    ///
    /// pg.zig's `Result.drain` only early-returns when `conn._state == .idle`.
    /// That misses a real case: `result.next()` returning null inside an
    /// explicit transaction consumes the final 'Z' message; pg.zig's
    /// `Conn.read` sets `_state = .transaction` (because the Z reports
    /// transaction status 'T'), not `.idle`. Upstream drain then treats the
    /// connection as "still in flight" and blocks on a socket read that
    /// will never complete — postgres has nothing more to send.
    ///
    /// Mirroring the upstream check against `.query` (the only state where
    /// there genuinely IS more to drain) avoids the deadlock without
    /// reaching into pg.zig internals beyond the `_state` field every
    /// caller already touches indirectly.
    pub fn drain(self: *PgQuery) void {
        if (self.inner._conn._state != .query) return;
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
