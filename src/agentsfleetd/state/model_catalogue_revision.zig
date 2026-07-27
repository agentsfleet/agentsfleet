//! The catalogue generation counter (§2).
//!
//! One row, read on every catalogue request and incremented by every admin
//! mutation. It exists so the response cache and the billing rate cache cannot
//! drift apart: each is internally consistent on its own, so a mismatch between
//! them is invisible without a shared generation to compare against.
//!
//! ## The two operations are asymmetric on purpose
//!
//! `read` is a plain SELECT on the hot path — no lock, because a reader only
//! needs *a* consistent generation, not the newest one. Reading N while a
//! mutation commits N+1 is fine: the reader serves a coherent page built from
//! generation N, and the next request reads N+1 and looks up a different cache
//! key. Taking a lock here would serialize every catalogue read behind the
//! occasional admin write for no correctness gain.
//!
//! `bumpLocked` takes `FOR UPDATE` because two concurrent mutations must not
//! both read N and both write N+1 — that would leave two different catalogue
//! states sharing one generation, which is precisely the drift the counter
//! exists to prevent. The lock is held for the whole mutation, so the caller's
//! catalogue change and the increment commit together or not at all.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("model_library/sql.zig");

const log = logging.scoped(.state);

pub const Error = error{
    /// The singleton row is missing. `schema/037` seeds it, so this means the
    /// row was deleted — the catalogue cannot select a cache generation, and the
    /// caller answers `UZ-LIBRARY-004` (503) rather than serving data from an
    /// unknown generation.
    RevisionMissing,
};

/// Current generation. Callers read this AFTER authentication and BEFORE
/// selecting a cache entry, so the revision they cache under is the one they
/// actually observed.
pub fn read(conn: *pg.Conn) (Error || anyerror)!i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_REVISION, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return Error.RevisionMissing;
    return row.get(i64, 0);
}

/// Take the generation lock inside the caller's open transaction.
///
/// Returns the current revision while holding it. The caller then mutates the
/// catalogue and calls `bumpLocked` before committing, so the new catalogue
/// state and its generation become visible in the same instant.
pub fn lock(conn: *pg.Conn) (Error || anyerror)!i64 {
    var q = PgQuery.from(try conn.query(sql.LOCK_REVISION, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return Error.RevisionMissing;
    return row.get(i64, 0);
}

/// Increment the generation. Must be called with `lock` already held in the same
/// transaction; the returned value is the generation the mutation produced.
pub fn bumpLocked(conn: *pg.Conn, now_ms: i64) (Error || anyerror)!i64 {
    var q = PgQuery.from(try conn.query(sql.BUMP_REVISION, .{now_ms}));
    defer q.deinit();
    const row = (try q.next()) orelse return Error.RevisionMissing;
    return row.get(i64, 0);
}

/// An open catalogue mutation, holding the generation lock.
///
/// Exactly one of `commitBumped` or `abort` must run. `abort` is idempotent, so
/// the shape is `defer txn.abort()` immediately after `beginMutation`, then
/// `txn.commitBumped(now_ms)` on the success path — the deferred abort no-ops
/// once the commit closed the transaction.
///
/// **`defer`, not `errdefer`.** Every admin handler that opens one of these
/// returns `void`, and an `errdefer` in a function that cannot return an error
/// never fires. §1's reference transaction shipped exactly that mistake: the
/// rollback every reader saw as present had never once run, and the
/// commit-failure path had none at all.
///
/// This type exists so the three catalogue mutations cannot each spell the
/// protocol themselves. A mutation that changed rows without bumping would leave
/// every replica's response cache keyed on a generation that no longer describes
/// the catalogue — and nothing would report it, because each cache stays
/// internally consistent.
pub const Txn = struct {
    const Self = @This();

    conn: *pg.Conn,
    open: bool,
    /// The generation observed under the lock. The caller's change becomes
    /// visible at `revision + 1`.
    revision: i64,

    /// Increment the generation and commit, so the catalogue change and the
    /// generation describing it become visible in the same instant. Returns the
    /// generation this mutation produced.
    pub fn commitBumped(self: *Self, now_ms: i64) !i64 {
        const next = try bumpLocked(self.conn, now_ms);
        _ = try self.conn.exec("COMMIT", .{});
        self.open = false;
        return next;
    }

    /// Roll back, swallowing the rollback's own failure. `conn.rollback()`
    /// rather than `exec("ROLLBACK")` because `exec` short-circuits once the
    /// connection is in FAIL state.
    pub fn abort(self: *Self) void {
        if (!self.open) return;
        self.open = false;
        self.conn.rollback() catch |err| log.warn("catalogue_rollback_failed", .{ .err = @errorName(err) });
    }
};

/// Open a catalogue mutation and take the generation lock.
///
/// On any failure the transaction is rolled back before returning, so a caller
/// that never receives a `Txn` has nothing to clean up.
pub fn beginMutation(conn: *pg.Conn) (Error || anyerror)!Txn {
    _ = try conn.exec("BEGIN", .{});
    var txn = Txn{ .conn = conn, .open = true, .revision = 0 };
    errdefer txn.abort();
    txn.revision = try lock(conn);
    return txn;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "the read path takes no lock and the mutation path does" {
    // Not a tautology: which statement carries FOR UPDATE is the entire
    // concurrency argument for this module, and swapping them would either
    // serialize every catalogue read behind admin writes (read locking) or let
    // two mutations share a generation (mutation not locking). Neither shows up
    // as a test failure anywhere else.
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_REVISION, "FOR UPDATE") == null);
    try testing.expect(std.mem.indexOf(u8, sql.LOCK_REVISION, "FOR UPDATE") != null);
}

test "the bump increments rather than assigning a caller-supplied value" {
    // `revision = revision + 1` is computed by the database under the row lock.
    // A caller-supplied next value would be read-modify-write across the
    // application boundary, which is exactly the lost-update the lock prevents.
    try testing.expect(std.mem.indexOf(u8, sql.BUMP_REVISION, "revision = revision + 1") != null);
    try testing.expect(std.mem.indexOf(u8, sql.BUMP_REVISION, "RETURNING revision") != null);
}

test "every statement addresses the singleton by its constrained id" {
    // The table is constrained to one row; a statement that forgot the WHERE
    // would still pass today and silently become a full-table write the moment
    // anyone widened the table.
    for ([_][]const u8{ sql.SELECT_REVISION, sql.LOCK_REVISION, sql.BUMP_REVISION }) |stmt| {
        try testing.expect(std.mem.indexOf(u8, stmt, "id = 1") != null);
    }
}
