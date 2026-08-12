//! Transaction-scoped role elevation — the one way any path reaches the
//! secret store, the wallet, or the fenced metering statement.
//!
//! `api_runtime` holds no privilege on `vault.secrets` or
//! `billing.tenant_wallet` (schema/300, schema/700); membership in the roles
//! that do is granted `WITH INHERIT FALSE, SET TRUE` (schema/110, schema/120),
//! so the privileges are dormant until a path names them with `SET ROLE`.
//! This module is that naming:
//!
//!   - **The typed handle states the requirement.** `Elevated(.vault)` is a
//!     distinct type per role, produced by an open scope. A function that
//!     issues vault SQL takes `Elevated(.vault)`, so the privilege it needs is
//!     visible in its signature and an unelevated caller has nothing to pass.
//!     PostgreSQL is the enforcement — it refuses the unelevated statement
//!     outright. The type is what moves that refusal from production to the
//!     call site; it is not itself the boundary.
//!   - **`defer` is the scope.** `begin` opens, `commit` closes, and a
//!     `deinit` that runs on every path undoes an uncommitted scope. This is
//!     the same shape `state/secret_reference_txn.zig` uses, and the reason it
//!     replaced a callback form: a callback cannot hand an owned value back
//!     without an out-parameter, because its epilogue (COMMIT) can fail after
//!     the body succeeded. Every allocating caller had to thread that by hand.
//!     A scope leaves the value an ordinary local.
//!   - `SET LOCAL ROLE` rather than `SET ROLE`: the server itself reverts the
//!     role at COMMIT or ROLLBACK. This is the divergence from the older
//!     memory-handler path (`handlers/memory/helpers.zig`), whose
//!     connection-scoped SET ROLE + paired RESET is exactly the
//!     convention-not-structure shape this module retires for money and
//!     secrets.
//!   - A scope opened on a connection already inside an explicit transaction
//!     elevates in place and steps back down with `SET LOCAL ROLE NONE` — the
//!     signup starter grant and the secret reference protocol mix `core.*`
//!     statements (as `api_runtime`) with elevated ones inside one atomic
//!     transaction. Outside a transaction the scope owns one, because
//!     `SET LOCAL` without a transaction is a warning and a no-op.
//!
//! Every open elevation is tracked by connection identity in
//! `pool_elevation_tracker.zig`; `pool.zig`'s release consults `auditRelease`
//! as the backstop, so a connection that somehow escapes its scope still
//! elevated is refused back into the pool rather than reused. RULE OWN: one
//! scope owns the elevation; a nested `begin` on the same connection is
//! refused, never stacked.

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

pub const Error = tracker.Error;

/// Proof, carried in the type, that `conn` is elevated to `role` for the
/// current transaction. Obtained from an open `Scope`. Hand-constructing one
/// in production code is a review defect — Zig cannot seal the literal, and
/// the database refuses the statement anyway, so the value here is that the
/// requirement is legible in every signature that needs it.
pub fn Elevated(comptime role: Role) type {
    return struct {
        conn: *pg.Conn,

        pub const elevated_role = role;

        // The handle is one pointer, passed by value.
        comptime {
            std.debug.assert(@sizeOf(@This()) == 8);
        }
    };
}

/// An open elevation. Obtained from `begin`, closed by `commit`, and undone by
/// `deinit` on any path that did not commit.
///
/// The three-line shape every caller uses:
///
///     var scope = try pool_elevation.begin(conn, .vault);
///     defer scope.deinit();
///     ... statements via scope.handle() / scope.conn ...
///     try scope.commit();
///
/// `deinit` after a successful `commit` is a no-op, so the `defer` is always
/// correct and never conditional.
pub fn Scope(comptime role: Role) type {
    return struct {
        const Self = @This();

        conn: *pg.Conn,
        /// True when `begin` opened the transaction and therefore owns ending
        /// it. False when the caller was already inside one: the scope only
        /// steps the role down and leaves COMMIT/ROLLBACK to its owner.
        owns_txn: bool,
        closed: bool = false,

        /// The typed proof to pass to functions that require this role.
        pub fn handle(self: Self) Elevated(role) {
            return .{ .conn = self.conn };
        }

        /// Close the scope successfully: COMMIT when this scope owns the
        /// transaction, otherwise step the role back down and leave the
        /// caller's transaction open.
        pub fn commit(self: *Self) !void {
            if (self.closed) return;
            // Marked closed BEFORE the fallible statement: if COMMIT fails the
            // transaction is over either way, and `deinit` must not then issue
            // a rollback against a connection whose transaction already ended.
            self.closed = true;
            defer tracker.unmark(self.conn);
            if (self.owns_txn) {
                try self.conn.commit();
            } else {
                _ = try self.conn.exec(S_SET_LOCAL_ROLE_NONE, .{});
            }
        }

        /// Undo an uncommitted scope. Safe to call after `commit` (no-op) and
        /// on error paths, which is what makes the `defer` unconditional.
        ///
        /// A step-down that cannot be delivered inside an already-aborted
        /// outer transaction is harmless: the server reverts the role when
        /// that transaction ends.
        pub fn deinit(self: *Self) void {
            if (self.closed) return;
            self.closed = true;
            defer tracker.unmark(self.conn);
            if (self.owns_txn) {
                self.conn.rollback() catch |err|
                    log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
            } else {
                _ = self.conn.exec(S_SET_LOCAL_ROLE_NONE, .{}) catch |err|
                    log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
            }
        }
    };
}

/// Open an elevation scope on `conn` for `role`.
///
/// Inside an explicit transaction the caller opened: elevates in place. Outside
/// one: opens a transaction this scope owns. A connection mid-query, failed, or
/// already elevated is refused under `UZ-INTERNAL-004`.
pub fn begin(conn: *pg.Conn, comptime role: Role) !Scope(role) {
    const in_txn = switch (conn._state) {
        .transaction => true,
        .idle => false,
        else => {
            logRefusal(conn, comptime role.dbName());
            return Error.ElevationRefused;
        },
    };

    tracker.mark(conn, comptime role.dbName()) catch |err| {
        logRefusal(conn, comptime role.dbName());
        return err;
    };
    errdefer tracker.unmark(conn);

    if (!in_txn) try conn.begin();
    errdefer if (!in_txn) conn.rollback() catch |err|
        log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });

    _ = try conn.exec(comptime setLocalStatement(role), .{});
    return .{ .conn = conn, .owns_txn = !in_txn };
}

fn logRefusal(conn: *pg.Conn, role_name: []const u8) void {
    log.err(EVENT_ELEVATION_REFUSED, .{
        .role = role_name,
        .conn_state = @tagName(conn._state),
        .error_code = error_codes.ERR_INTERNAL_DB_ELEVATION_REFUSED,
    });
}

/// Pool-release backstop — see `pool_elevation_tracker.auditRelease`.
pub fn auditRelease(conn: *pg.Conn) ?[]const u8 {
    return tracker.auditRelease(conn);
}

/// Operator-facing count of connections refused at release.
pub fn refusedReleaseCount() u64 {
    return tracker.refusedReleaseCount();
}

/// Operator-facing count of elevations refused by a full tracking table.
pub fn refusedMarkCount() u64 {
    return tracker.refusedMarkCount();
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

test "Elevated handles are distinct types per role, one pointer wide" {
    // The whole point of the typestate: a vault handle is not a billing
    // handle, so the compiler refuses a cross-domain pass.
    try testing.expect(Elevated(.vault) != Elevated(.billing));
    try testing.expect(Elevated(.vault).elevated_role == .vault);
    try testing.expectEqual(@as(usize, 8), @sizeOf(Elevated(.metering)));
}

test "a Scope hands out the handle for its own role only" {
    // Structural: `handle()` returns exactly `Elevated(role)`, so a scope
    // opened for billing cannot satisfy a vault signature.
    try testing.expectEqual(Elevated(.billing), @typeInfo(@TypeOf(Scope(.billing).handle)).@"fn".return_type.?);
    try testing.expectEqual(Elevated(.vault), @typeInfo(@TypeOf(Scope(.vault).handle)).@"fn".return_type.?);
}

test {
    _ = @import("schema_privilege_test.zig");
    // The already-imported module, not a second literal path to it.
    _ = tracker;
}
