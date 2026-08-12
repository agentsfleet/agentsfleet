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
//!     transaction elevates in place and steps back down with
//!     `SET LOCAL ROLE NONE` — the signup starter grant and the secret
//!     reference protocol mix `core.*` statements (as `api_runtime`) with
//!     elevated ones inside one atomic transaction. Outside a transaction the
//!     callback owns one, because `SET LOCAL` without a transaction is a
//!     warning and a no-op.
//!
//! Every open elevation is additionally tracked by connection identity;
//! `pool.zig`'s release consults `auditRelease` as the belt-and-braces
//! backstop, so a connection that somehow escapes its callback still elevated
//! is refused back into the pool (destroyed and counted, never reused).
//! RULE OWN: one callback owns the elevation; a nested `withRole` on the same
//! connection is refused, never stacked.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

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

/// The capability handle: proof, carried in the type, that `conn` is elevated
/// to `role` for the current transaction. Constructed ONLY by `withRole` —
/// treat a hand-rolled construction in production code as a review defect
/// (Zig cannot seal the struct literal; the signature contract is the gate).
pub fn Elevated(comptime role: Role) type {
    return struct {
        conn: *pg.Conn,

        pub const elevated_role = role;

        // bvisor pattern: the handle is one pointer, passed by value.
        comptime {
            std.debug.assert(@sizeOf(@This()) == 8);
        }
    };
}

pub const Error = error{
    /// The connection is already elevated (nesting is refused, RULE OWN), is in
    /// a failed or mid-query state, or the tracking table is full.
    ElevationRefused,
};

/// Deterministic bound on concurrently elevated connections. At most one
/// elevation is open per connection, so the ceiling that matters is pool size
/// — which is env-tunable up to the u16 maximum, NOT the default. Sizing this
/// against the default would refuse elevations on any large deployment, and a
/// refusal here surfaces as a 500 with no obvious cause, so the table is
/// generous and its exhaustion is counted (`refusedMarkCount`) rather than
/// silent.
const MAX_TRACKED_ELEVATIONS = 1024;

const Entry = struct { conn: *pg.Conn, role: Role };

// One mutex, protecting exactly `g_elevated`: the fixed table of connections
// currently inside an elevation callback. Compared by pointer identity only —
// nothing here dereferences the connection.
var g_mutex: common.Mutex = .{};
var g_elevated: [MAX_TRACKED_ELEVATIONS]?Entry = [_]?Entry{null} ** MAX_TRACKED_ELEVATIONS;
var g_refused_releases = std.atomic.Value(u64).init(0);
var g_refused_marks = std.atomic.Value(u64).init(0);

fn mark(conn: *pg.Conn, role: Role) Error!void {
    g_mutex.lock();
    defer g_mutex.unlock();
    var free_slot: ?usize = null;
    for (&g_elevated, 0..) |entry, i| {
        if (entry) |e| {
            if (e.conn == conn) return Error.ElevationRefused;
        } else if (free_slot == null) {
            free_slot = i;
        }
    }
    const slot = free_slot orelse {
        // Table pressure, not misuse — counted separately from the nesting
        // refusal above so an operator can tell the two apart.
        _ = g_refused_marks.fetchAdd(1, .monotonic);
        return Error.ElevationRefused;
    };
    g_elevated[slot] = .{ .conn = conn, .role = role };
}

fn unmark(conn: *pg.Conn) void {
    g_mutex.lock();
    defer g_mutex.unlock();
    for (&g_elevated) |*entry| {
        if (entry.*) |e| {
            if (e.conn == conn) {
                entry.* = null;
                return;
            }
        }
    }
}

/// Pool-release backstop. Returns the role a still-open elevation held
/// (clearing it and counting the refusal), or null for the normal, unelevated
/// release. The caller (`pool.zig`) destroys the connection instead of
/// pooling it.
pub fn auditRelease(conn: *pg.Conn) ?Role {
    g_mutex.lock();
    defer g_mutex.unlock();
    for (&g_elevated) |*entry| {
        if (entry.*) |e| {
            if (e.conn == conn) {
                entry.* = null;
                _ = g_refused_releases.fetchAdd(1, .monotonic);
                log.err("elevated_release_refused", .{
                    .role = e.role.dbName(),
                    .error_code = error_codes.ERR_INTERNAL_DB_ELEVATED_RELEASE,
                });
                return e.role;
            }
        }
    }
    return null;
}

/// Operator-facing count of connections refused at release (count only, no
/// identity).
pub fn refusedReleaseCount() u64 {
    return g_refused_releases.load(.monotonic);
}

/// Operator-facing count of elevations refused because the tracking table was
/// full. Non-zero means the table is undersized for the deployment's pool, so
/// the pressure is visible instead of arriving as unexplained 500s.
pub fn refusedMarkCount() u64 {
    return g_refused_marks.load(.monotonic);
}

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

/// Run `f(ctx, handle)` with `conn` elevated to `role` for exactly one
/// transaction.
///
/// In an explicit transaction already (`BEGIN` issued by the caller):
/// elevates in place; the role steps down with `SET LOCAL ROLE NONE` when `f`
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
        else => {
            log.err(EVENT_ELEVATION_REFUSED, .{
                .role = comptime role.dbName(),
                .conn_state = @tagName(conn._state),
                .error_code = error_codes.ERR_INTERNAL_DB_ELEVATION_REFUSED,
            });
            return Error.ElevationRefused;
        },
    };

    mark(conn, role) catch |err| {
        log.err(EVENT_ELEVATION_REFUSED, .{
            .role = comptime role.dbName(),
            .conn_state = @tagName(conn._state),
            .error_code = error_codes.ERR_INTERNAL_DB_ELEVATION_REFUSED,
        });
        return err;
    };
    // Single owner for the unmark: every exit path below runs it exactly once.
    defer unmark(conn);

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

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

fn fakeConn(comptime addr: usize) *pg.Conn {
    // Registry entries are compared by pointer identity and never dereferenced,
    // so an aligned dummy address stands in for a connection.
    return @ptrFromInt(std.mem.alignForward(usize, addr, @alignOf(pg.Conn)));
}

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

test "a second elevation on the same connection is refused, a different one is not" {
    const a = fakeConn(0x10000);
    const b = fakeConn(0x20000);
    try mark(a, .vault);
    defer unmark(b);
    try testing.expectError(Error.ElevationRefused, mark(a, .billing));
    try mark(b, .billing);
    unmark(a);
    try mark(a, .metering);
    unmark(a);
}

test "auditRelease clears the entry, counts the refusal, and is one-shot" {
    const c = fakeConn(0x30000);
    const before = refusedReleaseCount();
    try mark(c, .billing);
    const hit = auditRelease(c);
    try testing.expect(hit != null);
    try testing.expectEqual(Role.billing, hit.?);
    try testing.expectEqual(before + 1, refusedReleaseCount());
    // Cleared: a second audit of the same connection is the normal path.
    try testing.expect(auditRelease(c) == null);
    try testing.expectEqual(before + 1, refusedReleaseCount());
}

test {
    _ = @import("schema_privilege_test.zig");
}
