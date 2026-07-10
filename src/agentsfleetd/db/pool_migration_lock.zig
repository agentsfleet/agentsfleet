//! Migration advisory-lock acquisition. Split from `pool_migrations.zig`
//! (RULE FLL — that file sits at the 350-line cap) so the lock concern and
//! its fail-fast retry have one home.
//!
//! `acquire` is BOUNDED on purpose. A blocking `pg_advisory_lock` waits forever
//! if another session holds the lock (a crashed/concurrent migrator that never
//! released it), silently hanging `agentsfleetd migrate` until the deploy
//! machine times out (~5 min). Instead we poll `pg_try_advisory_lock` a bounded
//! number of times, then fail loudly with `error.MigrationLockUnavailable` so
//! the operator sees "lock held" in seconds. The bound is the stop path — there
//! is no unbounded wait.
//!
//! Advisory locks are session-scoped, so the migrator MUST use a direct/session
//! Postgres endpoint (PlanetScale `:5432`), NOT the transaction-pooled `:6432`
//! endpoint where the lock can't be held across statements. That's a connection
//! config (preflight playbook), not something this module detects.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("pg_query.zig").PgQuery;

const log = logging.scoped(.db_migrate);

const Conn = pg.Conn;

/// Single global key for the schema-migration advisory lock.
const AdvisoryLockKey: i64 = 0x7A6F6D6269650001;

/// Bounded retry: at most MAX_ATTEMPTS polls spaced RETRY_MS apart
/// (~30s worst case) before giving up. `pub` so `runMigrations` can delegate
/// to `runMigrationsBounded` with the production bound.
pub const MAX_ATTEMPTS: u32 = 30;
pub const RETRY_MS: u64 = 1_000;

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
/// Internal to the bounded-acquire write path; the read-side availability check
/// is `probeAvailable`.
fn tryAcquire(conn: *Conn) !bool {
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
///
/// A query error from `tryAcquire` (connection drop, network blip) is
/// DELIBERATELY fatal: it propagates immediately rather than counting as a
/// retryable attempt. The bound retries lock *contention* only — a dead
/// connection is not something more polling fixes, and failing fast on it
/// surfaces the real fault instead of masking it for ~30s.
pub fn acquireBounded(conn: *Conn, max_attempts: u32, retry_ms: u64) !void {
    var attempt: u32 = 1;
    while (true) : (attempt += 1) {
        const got = try tryAcquire(conn);
        switch (classifyAttempt(got, attempt, max_attempts)) {
            .acquired => {
                if (attempt > 1) log.info("migrate.lock_acquired_after_contention", .{ .attempt = attempt });
                return;
            },
            .exhausted => {
                log.warn("migrate.lock_exhausted", .{ .attempts = max_attempts, .waited_ms = max_attempts * retry_ms });
                return error.MigrationLockUnavailable;
            },
            .retry => {
                log.warn("migrate.lock_contended", .{ .attempt = attempt, .max_attempts = max_attempts, .retry_ms = retry_ms });
                common.sleepNanos(retry_ms * std.time.ns_per_ms);
            },
        }
    }
}

/// Release the lock. Best-effort: an unlock failure on a dropped connection
/// is not actionable (the session — and its lock — are already gone).
pub fn release(conn: *Conn) void {
    var result = PgQuery.from(conn.query("SELECT pg_advisory_unlock($1)", .{AdvisoryLockKey}) catch return);
    result.deinit();
}

/// Read-side availability check for `inspectMigrationState`, which runs over the
/// pooled API connection at serve-boot. Uses `pg_try_advisory_xact_lock`, which
/// auto-releases when the single statement's implicit transaction ends — so it
/// never leaves a lock behind on a pooled backend the way a session-scoped
/// acquire + separate unlock would. Advisory locks are cluster-wide, so it still
/// returns false when the (direct-connection) migrator holds the lock mid-run.
/// Returns true when the migration lock is currently free.
pub fn probeAvailable(conn: *Conn) !bool {
    var result = PgQuery.from(try conn.query("SELECT pg_try_advisory_xact_lock($1)", .{AdvisoryLockKey}));
    defer result.deinit();
    const row = try result.next() orelse return false;
    return row.get(bool, 0);
}
