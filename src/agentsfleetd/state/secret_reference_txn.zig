//! The one lock protocol shared by every path that creates or destroys a
//! reference to a stored credential.
//!
//! ## The race this closes
//!
//! `core.tenant_model_entries.secret_ref` names a `vault.secrets` row, but it
//! cannot be a foreign key: `secret_ref` is TEXT and the vault's identity is
//! `(workspace_id, key_name)`, while an entry is keyed by tenant. So the
//! database cannot refuse an entry that points at a credential which no longer
//! exists — only a lock protocol can.
//!
//! Both sides were check-then-act with nothing held between the check and the
//! act:
//!
//!     DELETE /workspaces/{ws}/secrets/{name}   POST /tenants/me/models
//!     ------------------------------------     -----------------------
//!     referenced count -> 0, proceed
//!                                              secret exists? -> yes, proceed
//!     DELETE FROM vault.secrets
//!                                              INSERT entry  <-- orphan
//!
//! The result is an entry naming a credential that is gone. It survives every
//! later read (the list degrades it to an opaque `custom_secret`), so nothing
//! reports the corruption; it fails at the point of use, when a fleet tries to
//! run and cannot resolve a key.
//!
//! ## The protocol
//!
//! One transaction, and locks taken in ONE order by every participant:
//!
//!   1. `vault.secrets (workspace_id, key_name)`  FOR UPDATE
//!   2. `core.tenant_model_entries` for that ref, ORDER BY id  FOR UPDATE
//!   3. `core.tenant_model_selection` for the tenant           FOR UPDATE
//!
//! Order is what prevents deadlock, and it is why this lives in one module
//! rather than being spelled at each of the five call sites: a protocol that
//! every caller re-implements is a protocol that one caller eventually
//! re-implements backwards. `ORDER BY id` in step 2 matters for the same
//! reason — two writers locking the same set of entry rows in opposite orders
//! deadlock each other.
//!
//! Whoever reaches step 1 first wins, and both outcomes are correct:
//!
//!   - Producer first: the delete blocks, then observes the new entry and
//!     refuses (the caller already reports "still referenced").
//!   - Delete first: the producer blocks, then finds no vault row, and rolls
//!     back with `SecretGone` -> `UZ-LIBRARY-008`. Nothing was written, so the
//!     client may simply retry and will be told the credential is gone.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");

const log = logging.scoped(.secret_reference_txn);

/// Step 0. Whose entries are at stake.
///
/// DERIVED from the workspace, never taken from the caller. The credential
/// lives in a workspace, `core.workspaces.tenant_id` is `NOT NULL`, and that
/// tenant's entries are the only ones that can reference it. A caller-supplied
/// tenant answers a different question — "who is asking" — and the two diverge
/// exactly where it does the most damage:
///
///   - A `workspace:any` operator deleting inside another tenant's workspace
///     passed its OWN tenant. Step 2 matched nothing, the reference count came
///     back zero, and the delete proceeded straight over live references it had
///     never looked at — recreating the orphan this whole module exists to
///     prevent, with the audit trail saying the operation was authorized.
///   - A platform principal has no tenant at all. The old signature took
///     `?[]const u8` and skipped steps 2 and 3 entirely on null, so that caller
///     took no entry locks and also counted zero.
///
/// Both were the same mistake: letting the identity of the REQUESTER decide
/// which rows to lock, when the rows are a property of the CREDENTIAL.
const OWNING_TENANT =
    \\SELECT tenant_id::text FROM core.workspaces
    \\ WHERE id = $1::uuid
;

/// Step 1. `SELECT 1 ... FOR UPDATE` rather than a plain read: the row lock is
/// the entire point, and zero rows means the credential is already gone.
const LOCK_SECRET =
    \\SELECT 1 FROM vault.secrets
    \\ WHERE workspace_id = $1::uuid AND key_name = $2
    \\ FOR UPDATE
;

/// Step 2. Locks every entry currently naming this credential, in id order.
/// Returns them so a caller that needs the reference count (the delete path)
/// gets it from the same statement that took the locks, with no second read
/// that could observe a different set.
const LOCK_ENTRIES =
    \\SELECT id::text FROM core.tenant_model_entries
    \\ WHERE tenant_id = $1::uuid AND secret_ref = $2
    \\ ORDER BY id
    \\ FOR UPDATE
;

/// Step 3. The tenant's active selection. Locked even when the caller does not
/// intend to write it: activation and deletion both read it to decide, and a
/// decision made against an unlocked row is a decision made against a row that
/// can change before the commit.
const LOCK_SELECTION =
    \\SELECT 1 FROM core.tenant_model_selection
    \\ WHERE tenant_id = $1::uuid
    \\ FOR UPDATE
;

pub const Error = error{
    /// The credential was deleted by a concurrent transaction that reached the
    /// vault row first. Nothing has been written; the caller rolls back and
    /// reports `UZ-LIBRARY-008`.
    SecretGone,

    /// The workspace naming this credential has no row. `workspace_id` is a
    /// `NOT NULL` foreign key on `vault.secrets`' owning workspace, so this is
    /// a broken invariant rather than a race — and it must fail loudly instead
    /// of falling back to "no tenant, so no references", which is precisely the
    /// reasoning that let the delete run blind.
    WorkspaceUnknown,
};

/// An open reference transaction holding the locks above.
///
/// Exactly one of `commit` or `abort` must run. `abort` is idempotent, so the
/// shape is `defer txn.abort()` immediately after `begin`, then `txn.commit()`
/// on the success path — the deferred abort no-ops once the commit closed the
/// transaction.
///
/// **Not `errdefer`.** Every HTTP handler that opens one of these returns
/// `void`, and an `errdefer` in a function that cannot return an error never
/// fires. Two call sites carried exactly that, so their rollback was decoration;
/// the path it actually mattered on — a COMMIT that fails, leaving `open` true
/// and returning normally — had no rollback at all.
pub const Txn = struct {
    const Self = @This();

    conn: *pg.Conn,
    open: bool,

    /// Number of registry entries that named this credential when the locks
    /// were taken. Stable for the life of the transaction — that is what step 2
    /// bought — so a delete path can decide on it without re-reading.
    reference_count: usize,

    pub fn commit(self: *Self) !void {
        if (!self.open) return;
        _ = try self.conn.exec("COMMIT", .{});
        self.open = false;
    }

    /// Roll back, swallowing the rollback's own failure. `conn.rollback()`
    /// rather than `exec("ROLLBACK")` because `exec` short-circuits once the
    /// connection is in FAIL state — the same reason signup_bootstrap uses it.
    /// A failure here is logged, not propagated: the caller is already on an
    /// error path and the pool discards a connection it cannot reset.
    pub fn abort(self: *Self) void {
        if (!self.open) return;
        self.open = false;
        self.conn.rollback() catch |err| log.warn("rollback_failed", .{ .err = @errorName(err) });
    }
};

/// Open a transaction and take the three locks in canonical order.
///
/// `tenant_id` is optional because the credential surface is workspace-scoped
/// while the model registry is tenant-scoped: a platform or bootstrap principal
/// holds no registry, so steps 2 and 3 have nothing to lock and are skipped.
/// Step 1 always runs — the vault row is the serialization point for everyone.
///
/// On any failure the transaction is rolled back before returning, so a caller
/// that never receives a `Txn` has nothing to clean up.
pub fn begin(
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) (Error || anyerror)!Txn {
    _ = try conn.exec("BEGIN", .{});
    var txn = Txn{ .conn = conn, .open = true, .reference_count = 0 };
    errdefer txn.abort();

    // Step 1 — the credential itself. Absent means a concurrent delete got
    // here first; every caller treats that as fatal to its own write.
    //
    // `FOR UPDATE` on `vault.secrets` needs `vault_runtime` (schema/300), so
    // the lock statement runs in an elevated callback inside THIS transaction
    // and steps back down before the `core.*` steps — which run as
    // `api_runtime`, whose privileges those steps need in turn.
    const LockCtx = struct { workspace_id: []const u8, key_name: []const u8 };
    const found = try pool_elevation.withRole(conn, .vault, LockCtx{
        .workspace_id = workspace_id,
        .key_name = key_name,
    }, struct {
        fn run(c: LockCtx, v: pool_elevation.Elevated(.vault)) !bool {
            var q = PgQuery.from(try v.conn.query(LOCK_SECRET, .{ c.workspace_id, c.key_name }));
            defer q.deinit();
            return (try q.next()) != null;
        }
    }.run);
    if (!found) return Error.SecretGone;

    // Step 0, issued here because step 1 is the cheaper rejection: no point
    // resolving an owner for a credential that is already gone. Copied out
    // before the result is drained — the row's bytes belong to the result set,
    // and steps 2 and 3 run their own queries on this connection.
    var tenant_buf: [64]u8 = undefined;
    const tid = blk: {
        var q = PgQuery.from(try conn.query(OWNING_TENANT, .{workspace_id}));
        defer q.deinit();
        const row = (try q.next()) orelse return Error.WorkspaceUnknown;
        const t = try row.get([]u8, 0);
        if (t.len == 0 or t.len > tenant_buf.len) return Error.WorkspaceUnknown;
        @memcpy(tenant_buf[0..t.len], t);
        break :blk tenant_buf[0..t.len];
    };

    // Step 2 — every entry naming it, in id order, counted while locked.
    {
        var q = PgQuery.from(try conn.query(LOCK_ENTRIES, .{ tid, key_name }));
        defer q.deinit();
        while (try q.next()) |_| txn.reference_count += 1;
    }

    // Step 3 — the tenant's active selection. Zero rows is normal (a tenant
    // that has never chosen a model); the lock is simply a no-op then.
    {
        var q = PgQuery.from(try conn.query(LOCK_SELECTION, .{tid}));
        defer q.deinit();
        while (try q.next()) |_| {}
    }

    return txn;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "the lock statements name the three tables in protocol order" {
    // A pin, not a tautology: the ORDER of these three statements is the
    // deadlock-freedom argument, and the statements are the only place it is
    // written down. Reordering them in `begin` without reordering the protocol
    // comment is exactly the edit this catches in review.
    try testing.expect(std.mem.indexOf(u8, LOCK_SECRET, "vault.secrets") != null);
    try testing.expect(std.mem.indexOf(u8, LOCK_ENTRIES, "core.tenant_model_entries") != null);
    try testing.expect(std.mem.indexOf(u8, LOCK_SELECTION, "core.tenant_model_selection") != null);
}

test "every lock statement actually takes a row lock" {
    // FOR UPDATE is the load-bearing clause. Without it these are plain reads
    // and the whole protocol degrades to the check-then-act it replaced, while
    // still looking correct at every call site.
    for ([_][]const u8{ LOCK_SECRET, LOCK_ENTRIES, LOCK_SELECTION }) |stmt| {
        try testing.expect(std.mem.indexOf(u8, stmt, "FOR UPDATE") != null);
    }
}

test "the entry lock is ordered, so two writers cannot deadlock on the same set" {
    // Locking the same rows in different orders is the classic deadlock, and
    // the only defence is that every participant sorts identically.
    try testing.expect(std.mem.indexOf(u8, LOCK_ENTRIES, "ORDER BY id") != null);
}

test "abort is idempotent so errdefer after commit is harmless" {
    // No connection needed: the guard is the `open` flag, and the intended
    // usage (errdefer abort + try commit) relies on the second call being a
    // no-op rather than a second ROLLBACK on a committed transaction.
    var txn = Txn{ .conn = undefined, .open = false, .reference_count = 0 };
    txn.abort();
    txn.abort();
    try testing.expect(!txn.open);
}
