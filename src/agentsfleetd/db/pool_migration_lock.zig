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

/// Name of the session GUC the pooler probe writes and reads back. The VALUE is
/// a fresh random nonce per call (see `assertSessionConnection`) — never a
/// constant — so a stale value an earlier probe left on a pooled backend can't
/// match this call's readback and let a pooler slip through.
const SessionProbeSetting = "agentsfleet.migrate_probe";

/// Process-wide sequence so each pooler-probe nonce is unique within this
/// process; combined with the nanosecond clock it's unique across processes too.
var probe_seq: std.atomic.Value(u64) = std.atomic.Value(u64).init(0);

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
/// Internal to the bounded-acquire write path — the read-side availability
/// probe is `probeAvailable` (pooler-safe; this session-scoped pair is not).
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

/// Pooler-safe availability probe for `inspectMigrationState`. Uses
/// `pg_try_advisory_xact_lock`, which auto-releases when the implicit
/// autocommit transaction of this single statement ends — so it NEVER leaks
/// onto a pooled backend the way the session-scoped `tryAcquire` + `release`
/// pair does (acquire lands on one pooled backend, the separate `release`
/// statement routes to another and no-ops, orphaning the lock). Advisory
/// locks are cluster-wide, so this still returns false when another session
/// (a direct-connection migrator mid-run) holds the lock. Returns true when
/// the migration lock is currently free.
pub fn probeAvailable(conn: *Conn) !bool {
    var result = PgQuery.from(try conn.query("SELECT pg_try_advisory_xact_lock($1)", .{AdvisoryLockKey}));
    defer result.deinit();
    const row = try result.next() orelse return false;
    return row.get(bool, 0);
}

/// Pure verdict: did the probe value survive the round-trip? `pub` for the
/// unit test, which exercises it without a database.
pub fn probeIsSession(got: ?[]const u8, nonce: []const u8) bool {
    return got != null and std.mem.eql(u8, got.?, nonce);
}

/// Refuse to migrate over a transaction-mode pooler (e.g. PlanetScale Postgres
/// :6432). Write a session setting, then read it back in a separate statement:
/// a transaction pooler can route the read to a different backend that never
/// saw the write, so the value reads back absent and we fail with
/// `error.MigratorNotSessionScoped` — at the right place, with a clear name —
/// instead of letting the session advisory lock silently leak onto a pooled
/// backend and hang the deploy. Keeps per-migration transactions intact (no
/// single-txn constraint), so non-transactional DDL stays possible.
///
/// The nonce is FRESH per call (clock + counter), so a stale setting an earlier
/// probe left on a different pooled backend can't match this readback and let a
/// pooler slip through. The residual limit is BEST-EFFORT, not a proof: with a
/// single idle migrator client a pooler often hands the SAME backend back for
/// the readback, so the probe can still pass on a real pooler under low
/// concurrency. The load-bearing guard against the hang is the bounded `acquire`
/// (it fails fast rather than blocking); this probe is the early, clearly-named
/// signal, and the `:5432` config (preflight playbook) is the primary defense.
pub fn assertSessionConnection(conn: *Conn) !void {
    // Fresh per call: the wall clock makes it unique across processes/runs and a
    // process-wide counter makes it unique within this process, so a stale value
    // an earlier probe left on a pooled backend can never match this readback.
    const seq = probe_seq.fetchAdd(1, .monotonic);
    // 48 bytes fits two u64s in hex plus the separator with room to spare, so
    // NoSpace is unreachable in practice; `try` keeps zlint happy without a
    // bespoke panic for an impossible branch.
    var nonce_buf: [48]u8 = undefined;
    const nonce = try std.fmt.bufPrint(&nonce_buf, "{x}-{x}", .{ @as(u64, @intCast(common.clock.nowMillis())), seq });

    _ = try conn.exec("SELECT set_config($1, $2, false)", .{ SessionProbeSetting, nonce });
    var q = PgQuery.from(try conn.query("SELECT current_setting($1, true)", .{SessionProbeSetting}));
    defer q.deinit();
    const row = try q.next() orelse return error.MigratorNotSessionScoped;
    if (!probeIsSession(try row.get(?[]const u8, 0), nonce)) return error.MigratorNotSessionScoped;
}
