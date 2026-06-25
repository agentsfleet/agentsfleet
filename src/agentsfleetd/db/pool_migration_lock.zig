//! Migration advisory-lock acquisition. Split from `pool_migrations.zig`
//! (RULE FLL — that file sits at the 350-line cap) so the lock concern and
//! its fail-fast retry have one home.
//!
//! `acquire` is BOUNDED on purpose. A blocking `pg_advisory_lock` waits
//! forever if another session holds the lock — or if a transaction-pooled
//! backend leaked it (PlanetScale Postgres pools in transaction mode, which
//! breaks session-scoped advisory locks; the migrator must use a direct
//! connection). That silently hangs `agentsfleetd migrate` until the deploy
//! machine times out (~5 min). Instead we poll `pg_try_advisory_lock` a
//! bounded number of times, then fail loudly with
//! `error.MigrationLockUnavailable` so the operator sees "lock held" in
//! seconds. The bound is the stop path — there is no unbounded wait.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const PgQuery = @import("pg_query.zig").PgQuery;

const Conn = pg.Conn;

/// Single global key for the schema-migration advisory lock.
const AdvisoryLockKey: i64 = 0x7A6F6D6269650001;

/// Bounded retry: at most MAX_ATTEMPTS polls spaced RETRY_MS apart
/// (~30s worst case) before giving up.
const MAX_ATTEMPTS: u32 = 30;
const RETRY_MS: u64 = 1_000;

/// Probe nonce: a transaction pooler assigns a different backend per statement,
/// so a session setting written by one statement is gone by the next. We write
/// this and read it back to detect that before the advisory lock can leak.
const SessionProbeNonce = "agentsfleet-migrate-session-probe";

/// One poll's verdict. `pub` for the unit test, which exercises the pure
/// retry decision without a database.
pub const Outcome = enum { acquired, retry, exhausted };

/// Pure decision for a single acquisition poll — no I/O, unit-testable.
pub fn classifyAttempt(acquired: bool, attempt: u32, max_attempts: u32) Outcome {
    if (acquired) return .acquired;
    if (attempt >= max_attempts) return .exhausted;
    return .retry;
}

/// Non-blocking poll: returns true if this session now holds the lock.
pub fn tryAcquire(conn: *Conn) !bool {
    var result = PgQuery.from(try conn.query("SELECT pg_try_advisory_lock($1)", .{AdvisoryLockKey}));
    defer result.deinit();
    const row = try result.next() orelse return false;
    return row.get(bool, 0);
}

/// Acquire under the production bound; fails fast instead of hanging.
pub fn acquire(conn: *Conn) !void {
    return acquireBounded(conn, MAX_ATTEMPTS, RETRY_MS);
}

/// Acquire under an injected bound so tests fail fast (and so the production
/// path stays a one-liner over the same loop). `error.MigrationLockUnavailable`
/// once the bound is exhausted.
pub fn acquireBounded(conn: *Conn, max_attempts: u32, retry_ms: u64) !void {
    var attempt: u32 = 1;
    while (true) : (attempt += 1) {
        const got = try tryAcquire(conn);
        switch (classifyAttempt(got, attempt, max_attempts)) {
            .acquired => return,
            .exhausted => return error.MigrationLockUnavailable,
            .retry => common.sleepNanos(retry_ms * std.time.ns_per_ms),
        }
    }
}

/// Release the lock. Best-effort: an unlock failure on a dropped connection
/// is not actionable (the session — and its lock — are already gone).
pub fn release(conn: *Conn) void {
    var result = PgQuery.from(conn.query("SELECT pg_advisory_unlock($1)", .{AdvisoryLockKey}) catch return);
    result.deinit();
}

/// Pure verdict: did the probe value survive the round-trip? `pub` for the
/// unit test, which exercises it without a database.
pub fn probeIsSession(got: ?[]const u8, nonce: []const u8) bool {
    return got != null and std.mem.eql(u8, got.?, nonce);
}

/// Refuse to migrate over a transaction-mode pooler (e.g. PlanetScale Postgres
/// :6432). Write a session setting, then read it back in a separate statement:
/// a pooler routes the read to a different backend, so the value is gone. We
/// fail with `error.MigratorNotSessionScoped` — at the right place, with a
/// clear name — instead of letting the session advisory lock silently leak onto
/// a pooled backend and hang the deploy. Keeps per-migration transactions
/// intact (no single-txn constraint), so non-transactional DDL stays possible.
pub fn assertSessionConnection(conn: *Conn) !void {
    _ = try conn.exec("SELECT set_config('agentsfleet.migrate_probe', $1, false)", .{SessionProbeNonce});
    var q = PgQuery.from(try conn.query("SELECT current_setting('agentsfleet.migrate_probe', true)", .{}));
    defer q.deinit();
    const row = try q.next() orelse return error.MigratorNotSessionScoped;
    if (!probeIsSession(try row.get(?[]const u8, 0), SessionProbeNonce)) return error.MigratorNotSessionScoped;
}
