//! Transaction-scoped role elevation — the one way any path reaches the
//! secret store, the wallet, or the fenced metering statement.
//!
//! `api_runtime` holds no privilege on `vault.secrets` or
//! `billing.tenant_wallet` (schema/300, schema/700); membership in the roles
//! that do is granted `WITH INHERIT FALSE, SET TRUE` (schema/110, schema/120),
//! so the privileges are dormant until a path names them with `SET ROLE`.
//! This module is that naming, shaped so misuse is a compile error rather
//! than a runtime refusal:
//!
//!   - **The typed handle is the proof.** `Elevated(.vault)` is a distinct
//!     type per role, constructed only inside `withRole`. A function that
//!     issues vault SQL takes `Elevated(.vault)`, so an unelevated caller
//!     fails to compile — the privilege requirement lives in the signature.
//!   - **The closure is the scope.** `withRole` brackets exactly one callback:
//!     BEGIN + `SET LOCAL ROLE` before it, COMMIT on success, ROLLBACK on
//!     error. There is no `finish` to forget and no guard object to leak —
//!     the shape Bun's `sql.begin(cb)` settled on for the same reason.
//!   - `SET LOCAL ROLE` rather than `SET ROLE`: the server itself reverts the
//!     role at COMMIT or ROLLBACK. This is the divergence from the older
//!     memory-handler path (`handlers/memory/helpers.zig`), whose
//!     connection-scoped SET ROLE + paired RESET is exactly the
//!     convention-not-structure shape this module retires for money and
//!     secrets.
//!   - A callback opened on a connection already inside an explicit
//!     transaction elevates in place and steps back down when it returns —
//!     the signup starter grant and the secret reference protocol mix `core.*`
//!     statements (as `api_runtime`) with elevated ones inside one atomic
//!     transaction. Outside a transaction the callback owns one, because
//!     `SET LOCAL` without a transaction is a warning and a no-op.
//!   - **The step-down is `SET LOCAL ROLE NONE`, which RESTORES
//!     `session_user`** — it does not name a role. Naming one
//!     (`SET LOCAL ROLE api_runtime`) was tried and is wrong: it *forces*
//!     rather than restores, so a session that entered the callback as
//!     something broader is silently downgraded for the rest of its
//!     transaction, and the unelevated statements that follow inside a mixed
//!     transaction (the purge's `core.*` deletes, the signup starter grant's
//!     continuation) lose privileges they held on the way in.
//!
//!     The invariant that makes `NONE` correct in production: every pool that
//!     elevates logs in AS the data-plane role (`DATABASE_URL_API`), so
//!     `session_user` IS `api_runtime`. Elevation is API-pool-only — migrations
//!     run on their own pool and never elevate. The integration suite is the
//!     one session where that does not hold (it connects as a superuser), so
//!     its post-callback statements run with more rights than production has;
//!     the refusal assertions that matter drop to `SET ROLE api_runtime`
//!     explicitly rather than relying on the step-down.
//!
//! Every open elevation is additionally tracked by connection identity;
//! `pool.zig`'s release consults `auditRelease` as the belt-and-braces
//! backstop, so a connection that somehow escapes its callback still elevated
//! is refused back into the pool (destroyed and counted, never reused).
//! RULE OWN: one callback owns the elevation; a nested `withRole` on the same
//! connection is refused, never stacked.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");
const tracker = @import("pool_elevation_tracker.zig");

const log = logging.scoped(.db_elevation);

const EVENT_ELEVATION_REFUSED = "elevation_refused";

/// The elevation roles this module may assume. `.memory` exists for the
/// account purge, whose transaction must erase `memory.memory_entries`; the
/// HTTP memory handlers keep their request-scoped helper
/// (`handlers/memory/helpers.zig`) and migrating them here is its own change.
pub const Role = enum {
    vault,
    billing,
    metering,
    memory,

    pub fn dbName(self: Role) []const u8 {
        return switch (self) {
            .vault => ROLE_NAME_VAULT,
            .billing => ROLE_NAME_BILLING,
            .metering => ROLE_NAME_METERING,
            .memory => ROLE_NAME_MEMORY,
        };
    }
};

// Role identifiers, shared verbatim with the schema slots that create them
// (schema/110, schema/120) and with the privilege unit test (RULE UFS).
pub const ROLE_NAME_VAULT = "vault_runtime";
pub const ROLE_NAME_BILLING = "billing_runtime";
pub const ROLE_NAME_METERING = "metering_runtime";
pub const ROLE_NAME_MEMORY = "memory_runtime";

/// The data-plane role every elevating pool logs in as (schema/110). Not an
/// elevation role — the one the elevation roles are reached FROM, and the one
/// `session_user` is expected to be wherever this module runs in production.
pub const ROLE_NAME_API = "api_runtime";

// Statements are comptime-composed: the role is an identifier, not a bindable
// parameter, and composing from the named constants keeps the grep surface one.
const S_SET_LOCAL_ROLE_PREFIX = "SET LOCAL ROLE ";
const S_SET_LOCAL_ROLE_NONE = S_SET_LOCAL_ROLE_PREFIX ++ "NONE";
fn setLocalStatement(comptime role: Role) []const u8 {
    return switch (role) {
        .vault => S_SET_LOCAL_ROLE_PREFIX ++ ROLE_NAME_VAULT,
        .billing => S_SET_LOCAL_ROLE_PREFIX ++ ROLE_NAME_BILLING,
        .metering => S_SET_LOCAL_ROLE_PREFIX ++ ROLE_NAME_METERING,
        .memory => S_SET_LOCAL_ROLE_PREFIX ++ ROLE_NAME_MEMORY,
    };
}

/// The capability handle: proof, carried in the type, that `conn` is elevated
/// to `role` for the current transaction. Constructed ONLY by `withRole` —
/// treat a hand-rolled construction in production code as a review defect
/// (Zig cannot seal the struct literal; the signature contract is the gate).
pub fn Elevated(comptime role: Role) type {
    return struct {
        conn: *pg.Conn,

        pub const elevated_role = role;

        // bvisor pattern: the handle is one pointer, passed by value — stated
        // against the pointer's own width so it holds on any target.
        comptime {
            std.debug.assert(@sizeOf(@This()) == @sizeOf(*pg.Conn));
        }
    };
}

/// Which connections are currently elevated lives in `pool_elevation_tracker`
/// — a separate concern with its own process-global table, re-exported here so
/// callers keep one import.
pub const Error = tracker.Error;
pub const auditRelease = tracker.auditRelease;
pub const refusedReleaseCount = tracker.refusedReleaseCount;

/// The payload type of `f`'s return, with its error union stripped —
/// `withRole` re-wraps it in `anyerror` because the bracketing statements
/// (BEGIN, SET LOCAL, COMMIT) contribute their own failures.
fn Payload(comptime f: anytype) type {
    const ret = @typeInfo(@TypeOf(f)).@"fn".return_type.?;
    return switch (@typeInfo(ret)) {
        .error_union => |eu| eu.payload,
        else => ret,
    };
}

/// Log and answer the one refusal this module raises. Both entry checks share
/// it so the operator sees the same fields whichever tripped.
fn refuse(conn: *pg.Conn, comptime role: Role) Error {
    log.err(EVENT_ELEVATION_REFUSED, .{
        .role = comptime role.dbName(),
        .conn_state = @tagName(conn._state),
        .error_code = error_codes.ERR_INTERNAL_DB_ELEVATION_REFUSED,
    });
    return Error.ElevationRefused;
}

/// Run `f(ctx, handle)` with `conn` elevated to `role` for exactly one
/// transaction.
///
/// In an explicit transaction already (`BEGIN` issued by the caller):
/// elevates in place; the role steps back down to `api_runtime` when `f`
/// returns, and the caller's COMMIT/ROLLBACK ends the transaction. Outside
/// one: this call owns the transaction — COMMIT when `f` succeeds, ROLLBACK
/// when it fails. A connection mid-query, failed, or already elevated is
/// refused under `UZ-INTERNAL-004`.
///
/// Failure handling mirrors `state/secret_reference_txn.zig`: rollback uses
/// `conn.rollback()` (exec short-circuits once the connection is in FAIL
/// state), and a step-down that cannot be delivered inside an aborted outer
/// transaction is harmless — the server reverts the role when that
/// transaction ends.
pub fn withRole(
    conn: *pg.Conn,
    comptime role: Role,
    ctx: anytype,
    comptime f: anytype,
) anyerror!Payload(f) {
    const in_txn = switch (conn._state) {
        .transaction => true,
        .idle => false,
        else => return refuse(conn, role),
    };
    tracker.mark(conn, comptime role.dbName()) catch return refuse(conn, role);
    // Single owner for the unmark: every exit path below runs it exactly once.
    defer tracker.unmark(conn);

    if (!in_txn) try conn.begin();
    errdefer if (!in_txn) conn.rollback() catch |err|
        log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });

    _ = try conn.exec(comptime setLocalStatement(role), .{});

    const result = f(ctx, Elevated(role){ .conn = conn }) catch |err| {
        if (in_txn) {
            // Best-effort step-down; if the statement that failed aborted the
            // outer transaction, exec short-circuits and the server reverts
            // the role at that transaction's ROLLBACK anyway.
            _ = conn.exec(S_SET_LOCAL_ROLE_NONE, .{}) catch |reset_err|
                log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(reset_err) });
        }
        return err;
    };

    if (in_txn) {
        _ = try conn.exec(S_SET_LOCAL_ROLE_NONE, .{});
    } else {
        try conn.commit();
    }
    return result;
}

/// One elevated statement, its affected-row count returned — the shape roughly
/// half the call sites have. Same bracket, same registry, same grep surface as
/// `withRole`; it exists because spelling a context struct and a callback for
/// a body that is one `exec` cost a dozen lines at each of those sites and
/// pushed several callers past the function-length cap.
///
/// Reads stay on `withRole`: a row must be consumed and drained inside the
/// callback, which is exactly the closure the callback form already is.
pub fn execAs(
    conn: *pg.Conn,
    comptime role: Role,
    statement: []const u8,
    args: anytype,
) !?i64 {
    const Ctx = struct { statement: []const u8, args: @TypeOf(args) };
    return withRole(conn, role, Ctx{ .statement = statement, .args = args }, struct {
        fn run(c: Ctx, v: Elevated(role)) !?i64 {
            return v.conn.exec(c.statement, c.args);
        }
    }.run);
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "role names match the schema slots verbatim" {
    try testing.expectEqualStrings("vault_runtime", Role.vault.dbName());
    try testing.expectEqualStrings("billing_runtime", Role.billing.dbName());
    try testing.expectEqualStrings("metering_runtime", Role.metering.dbName());
    try testing.expectEqualStrings("memory_runtime", Role.memory.dbName());
}

test "elevation statements are SET LOCAL, never session-scoped SET ROLE" {
    // SET LOCAL is the load-bearing keyword: session-scoped SET ROLE survives
    // COMMIT and is exactly the leak the pool backstop exists to catch.
    inline for ([_]Role{ .vault, .billing, .metering, .memory }) |r| {
        try testing.expect(std.mem.startsWith(u8, setLocalStatement(r), S_SET_LOCAL_ROLE_PREFIX));
    }
    try testing.expect(std.mem.startsWith(u8, S_SET_LOCAL_ROLE_NONE, S_SET_LOCAL_ROLE_PREFIX));
}

test "the step-down restores session_user rather than naming a role" {
    // Naming a role here forces instead of restores: a session that entered
    // broader than the named role is silently downgraded for the rest of its
    // transaction, and the unelevated statements that follow in a mixed
    // transaction lose privileges they held on the way in. NONE is the only
    // spelling that returns the connection to exactly what it was.
    try testing.expectEqualStrings("SET LOCAL ROLE NONE", S_SET_LOCAL_ROLE_NONE);
    inline for ([_]Role{ .vault, .billing, .metering, .memory }) |r| {
        try testing.expect(std.mem.indexOf(u8, S_SET_LOCAL_ROLE_NONE, r.dbName()) == null);
    }
    // The base role is not one of the elevation roles: it is what they are
    // reached FROM, so no `Role` tag may name it.
    inline for ([_]Role{ .vault, .billing, .metering, .memory }) |r| {
        try testing.expect(!std.mem.eql(u8, r.dbName(), ROLE_NAME_API));
    }
}

test "Elevated handles are distinct types per role, one pointer wide" {
    // The whole point of the typestate: a vault handle is not a billing
    // handle, so the compiler refuses a cross-domain pass.
    try testing.expect(Elevated(.vault) != Elevated(.billing));
    try testing.expect(Elevated(.vault).elevated_role == .vault);
    try testing.expectEqual(@sizeOf(*pg.Conn), @sizeOf(Elevated(.metering)));
}

test "every role's tracker name is the name it elevates with" {
    // The tracker stores names, not tags, so this is the seam that would drift
    // silently: an entry naming a role the SET LOCAL never issued would make
    // the release-refusal log point at the wrong privilege.
    inline for ([_]Role{ .vault, .billing, .metering, .memory }) |r| {
        try testing.expect(std.mem.endsWith(u8, setLocalStatement(r), r.dbName()));
    }
}

test {
    _ = @import("schema_privilege_test.zig");
    // Already imported at file scope; referencing that binding pulls the
    // tracker's own tests in without repeating its path (RULE UFS).
    _ = tracker;
}
