//! Integration tier for §1 Dimension 1.2 — the secret-reference lock protocol.
//!
//! `state/secret_reference_txn.zig` exists to close a check-then-act race: a
//! registry entry can name a `vault.secrets` row, but `secret_ref` is TEXT while
//! the vault's identity is `(workspace_id, key_name)`, so no foreign key can
//! refuse an orphan. Only the lock order can.
//!
//! A unit test cannot prove that. The protocol's whole claim is about what a
//! SECOND session observes while a first one holds a row lock, so this drives
//! two real connections against real `FOR UPDATE` locks and asserts both
//! interleavings end correctly:
//!
//!   - **Delete first** — the producer finds no vault row and rolls back with
//!     `SecretGone`. Nothing is written, so the client may retry and be told the
//!     credential is gone.
//!   - **Producer first** — the delete BLOCKS rather than proceeding, then sees
//!     the entry the producer committed and refuses.
//!
//! The second is the one worth the machinery. "The delete blocks" is exactly
//! what a missing `FOR UPDATE` would silently stop doing while every other test
//! still passed, and the failure it permits — an entry naming a deleted
//! credential — survives every later read and only surfaces when a fleet cannot
//! resolve a key.
//!
//! Blocking is asserted with `lock_timeout` rather than threads: the contending
//! statement is required to TIME OUT while the lock is held, and to succeed once
//! it is released. That is deterministic and cannot hang the suite.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const cp = @import("../secrets/crypto_primitives.zig");
const entries_state = @import("tenant_model_entries.zig");
const txn = @import("secret_reference_txn.zig");
const vault = @import("vault.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const TENANT = base.TEST_TENANT_ID;
const WORKSPACE = "0195b4ba-8d3a-7f13-8abc-0000000e0002";
const KEY_NAME = "reference-race-key";
const ENTRY_ID = "0195b4ba-8d3a-7f13-8abc-0000001f0001";
const MODEL_ID = "claude-race-model";

const SECRET_BODY =
    \\{"kind":"llm_provider","provider":"anthropic","api_key":"sk-race"}
;

/// Long enough that a genuinely-acquired lock is not mistaken for a timeout on a
/// loaded machine, short enough that the contended case does not stall the lane.
const LOCK_TIMEOUT = "SET lock_timeout = '400ms'";
const LOCK_TIMEOUT_OFF = "SET lock_timeout = 0";

/// `begin` issues its BEGIN before step 1 can refuse, so a refusal that does not
/// roll back hands the pool a connection still inside a transaction. That is the
/// operator-visible shape — a backend parked in `idle in transaction`, holding
/// its snapshot and blocking vacuum — so it is what this asserts, rather than
/// the presence of the rung in the source.
const SUBJECT_BACKEND_PID = "SELECT pg_backend_pid()";
const SUBJECT_IS_IN_TRANSACTION =
    \\SELECT state = 'idle in transaction'
    \\  FROM pg_stat_activity
    \\ WHERE pid = $1
;

fn backendPid(conn: *pg.Conn) !i32 {
    var q = PgQuery.from(try conn.query(SUBJECT_BACKEND_PID, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoBackendPid;
    return row.get(i32, 0);
}

/// Read from the OBSERVER connection: a subject asked about itself reports the
/// state of the very query doing the asking.
fn subjectLeftTransactionOpen(observer: *pg.Conn, subject_pid: i32) !bool {
    var q = PgQuery.from(try observer.query(SUBJECT_IS_IN_TRANSACTION, .{subject_pid}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.SubjectSessionMissing;
    return (try row.get(?bool, 0)) orelse false;
}

/// Step 1 of the protocol, issued directly by the contender so the test observes
/// the lock itself rather than a side effect of it.
const CONTEND_FOR_SECRET =
    \\SELECT 1 FROM vault.secrets
    \\ WHERE workspace_id = $1::uuid AND key_name = $2
    \\ FOR UPDATE
;

const TestDb = struct {
    pool: *pg.Pool,
    /// The producer's session.
    a: *pg.Conn,
    /// The deleter's session. A second real connection is the point — two
    /// statements on one connection serialize trivially and would prove nothing.
    b: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        const second = ctx.pool.acquire() catch {
            ctx.pool.release(ctx.conn);
            ctx.pool.deinit();
            return null;
        };
        return .{ .pool = ctx.pool, .a = ctx.conn, .b = second };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.b);
        self.pool.release(self.a);
        self.pool.deinit();
    }
};

fn seed(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspaceWithTenant(conn, WORKSPACE, TENANT);
    try vault.storeJsonPlaintext(alloc, conn, WORKSPACE, KEY_NAME, SECRET_BODY);
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid", .{TENANT}) catch |err|
        std.log.warn("entry wipe ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{WORKSPACE}) catch |err|
        std.log.warn("secret wipe ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, WORKSPACE);
}

/// Try to take the vault row lock under a timeout. Returns true when acquired
/// (and immediately rolls back so the caller holds nothing), false when the
/// attempt timed out because someone else holds it.
fn tryLockSecret(conn: *pg.Conn) !bool {
    _ = try conn.exec(LOCK_TIMEOUT, .{});
    // Restoring the default matters even on the failure path: this connection
    // goes back to the pool, and a lingering `lock_timeout` would make some
    // unrelated later test fail for a reason nothing in it explains.
    defer _ = conn.exec(LOCK_TIMEOUT_OFF, .{}) catch |err|
        std.log.warn("lock_timeout reset ignored: {s}", .{@errorName(err)});
    _ = try conn.exec("BEGIN", .{});

    const got = blk: {
        var q = PgQuery.from(conn.query(CONTEND_FOR_SECRET, .{ WORKSPACE, KEY_NAME }) catch break :blk false);
        defer q.deinit();
        _ = q.next() catch break :blk false;
        break :blk true;
    };
    // Whether it timed out or succeeded, this session must end holding nothing:
    // a lingering transaction would poison the pooled connection for the next
    // acquirer and turn a clean failure here into an unrelated one elsewhere.
    conn.rollback() catch |err| std.log.warn("contender rollback ignored: {s}", .{@errorName(err)});
    return got;
}

test "integration: test_secret_reference_paths_serialize: delete first makes the producer roll back" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.a);
    try seed(alloc, db.a);
    defer teardown(db.a);

    // The delete wins the race outright and commits.
    _ = try db.b.exec(
        "DELETE FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2",
        .{ WORKSPACE, KEY_NAME },
    );

    // The producer now opens its transaction and must refuse: step 1 finds no
    // row. `SecretGone` rather than a generic failure is what lets the handler
    // answer UZ-LIBRARY-008 instead of a 500.
    try std.testing.expectError(
        txn.Error.SecretGone,
        txn.begin(db.a, WORKSPACE, KEY_NAME),
    );

    // And nothing was written on the way to that refusal.
    const count = try entries_state.referencedSecretCount(db.a, TENANT, KEY_NAME);
    try std.testing.expectEqual(@as(i64, 0), count);

    // The refusal must also have CLOSED the transaction `begin` opened. Until
    // this assertion existed the `errdefer txn.abort()` rung was executed by
    // this very test and proven by nothing: delete the rung and everything
    // above still passed, because a count read inside a stray open transaction
    // returns 0 just as happily. A rung that runs is not a rung that works.
    const pid = try backendPid(db.a);
    try std.testing.expect(!(try subjectLeftTransactionOpen(db.b, pid)));
}

test "integration: test_secret_reference_paths_serialize: producer first makes the delete wait" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.a);
    try seed(alloc, db.a);
    defer teardown(db.a);

    // Baseline: with nobody holding the row, the contender takes it freely. This
    // is what makes the negative assertion below meaningful — without it, a
    // lock_timeout firing for an unrelated reason would read as success.
    try std.testing.expect(try tryLockSecret(db.b));

    // Producer opens the protocol and holds all three locks.
    var t = try txn.begin(db.a, WORKSPACE, KEY_NAME);
    errdefer t.abort();

    // The delete path's first step cannot proceed while that is open.
    try std.testing.expect(!(try tryLockSecret(db.b)));

    // The producer writes its reference and commits.
    try entries_state.ensureEntry(alloc, db.a, TENANT, MODEL_ID, KEY_NAME);
    try t.commit();

    // Released — and the delete, now able to look, sees the entry the producer
    // committed and therefore refuses rather than orphaning it.
    try std.testing.expect(try tryLockSecret(db.b));
    const count = try entries_state.referencedSecretCount(db.b, TENANT, KEY_NAME);
    try std.testing.expectEqual(@as(i64, 1), count);
}

test "integration: test_secret_reference_paths_serialize: the reference count is taken under the lock" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.a);
    try seed(alloc, db.a);
    defer teardown(db.a);

    var pre = try entries_state.create(alloc, db.a, .{
        .id = ENTRY_ID,
        .tenant_id = TENANT,
        .model_id = MODEL_ID,
        .secret_ref = KEY_NAME,
    });
    pre.deinit(alloc);

    // Step 2 counts the entries it locked, in the same statement that locked
    // them. A caller re-reading the count separately could observe a different
    // set than the one it holds locks on, which is the bug this shape avoids.
    var t = try txn.begin(db.a, WORKSPACE, KEY_NAME);
    defer t.abort();
    try std.testing.expectEqual(@as(usize, 1), t.reference_count);
}

test "integration: test_secret_reference_paths_serialize: a caller with no tenant of its own still counts the workspace's references" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.a);
    try seed(alloc, db.a);
    defer teardown(db.a);

    // This used to assert the opposite, and the opposite was the bug. The
    // protocol took the tenant from the CALLER, so a principal with none — a
    // platform operator, or a `workspace:any` holder acting inside someone
    // else's workspace — skipped steps 2 and 3 entirely and got a reference
    // count of zero. A delete then proceeded straight over live references it
    // had never looked at, producing the orphan this module exists to prevent.
    //
    // The tenant is now derived from the workspace, so who is asking cannot
    // change which rows are protected.
    var pre = try entries_state.create(alloc, db.a, .{
        .id = ENTRY_ID,
        .tenant_id = TENANT,
        .model_id = MODEL_ID,
        .secret_ref = KEY_NAME,
    });
    pre.deinit(alloc);

    var t = try txn.begin(db.a, WORKSPACE, KEY_NAME);
    defer t.abort();
    try std.testing.expectEqual(@as(usize, 1), t.reference_count);

    // It really is holding the row: a contender must still be excluded.
    try std.testing.expect(!(try tryLockSecret(db.b)));
}
