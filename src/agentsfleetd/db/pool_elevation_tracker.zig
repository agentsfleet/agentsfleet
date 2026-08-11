//! Which connections are currently inside an elevation callback.
//!
//! Split from `pool_elevation.zig` because it is a different concern with its
//! own process-global state: the elevation semantics decide what a role may
//! do, this table only answers "is this connection still elevated right now",
//! and `pool.zig` consults it on every release without caring about roles.
//!
//! Entries hold the role NAME rather than the `Role` tag on purpose — it keeps
//! this file free of any dependency on the elevation module, so the two import
//! in one direction only. The names are comptime string constants with static
//! lifetime, so the stored slice always outlives the entry.
//!
//! Connections are compared by pointer identity and NEVER dereferenced: an
//! entry may outlive the connection it names (that is precisely the leak the
//! release backstop exists to catch), so reading through the pointer would be
//! a use-after-free.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.db_elevation);

const EVENT_ELEVATED_RELEASE_REFUSED = "elevated_release_refused";

pub const Error = error{
    /// The connection is already elevated (nesting is refused, RULE OWN), is in
    /// a failed or mid-query state, or the tracking table is full.
    ElevationRefused,
};

/// Deterministic bound on concurrently elevated connections. A pool holds a
/// handful of connections (POOL_SIZE_DEFAULT = 4) and elevation lasts one
/// transaction, so 64 is headroom, not a budget anyone approaches; hitting it
/// refuses the elevation loudly rather than growing.
const MAX_TRACKED_ELEVATIONS = 64;

const Entry = struct { conn: *pg.Conn, role_name: []const u8 };

// One mutex, protecting exactly `g_elevated`: the fixed table of connections
// currently inside an elevation callback. The critical section is a bounded
// scan of pointer compares with no I/O, so it never becomes a wait point at
// pool sizes this process uses.
var g_mutex: common.Mutex = .{};
var g_elevated: [MAX_TRACKED_ELEVATIONS]?Entry = [_]?Entry{null} ** MAX_TRACKED_ELEVATIONS;
var g_refused_releases = std.atomic.Value(u64).init(0);

/// Claim `conn` for an elevation to `role_name`. Refuses a second claim on the
/// same connection (RULE OWN: one callback owns the elevation, never stacked)
/// and refuses when the table is full.
pub fn mark(conn: *pg.Conn, role_name: []const u8) Error!void {
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
    const slot = free_slot orelse return Error.ElevationRefused;
    g_elevated[slot] = .{ .conn = conn, .role_name = role_name };
}

/// Release the claim. Silent when absent: `auditRelease` may have cleared it.
pub fn unmark(conn: *pg.Conn) void {
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

/// Pool-release backstop. Returns the role name a still-open elevation held
/// (clearing it and counting the refusal), or null for the normal, unelevated
/// release. The caller (`pool.zig`) destroys the connection instead of
/// pooling it.
pub fn auditRelease(conn: *pg.Conn) ?[]const u8 {
    g_mutex.lock();
    defer g_mutex.unlock();
    for (&g_elevated) |*entry| {
        if (entry.*) |e| {
            if (e.conn == conn) {
                entry.* = null;
                _ = g_refused_releases.fetchAdd(1, .monotonic);
                log.err(EVENT_ELEVATED_RELEASE_REFUSED, .{
                    .role = e.role_name,
                    .error_code = error_codes.ERR_INTERNAL_DB_ELEVATED_RELEASE,
                });
                return e.role_name;
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

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

fn fakeConn(addr: usize) *pg.Conn {
    // Entries are compared by pointer identity and never dereferenced, so an
    // aligned dummy address stands in for a connection.
    return @ptrFromInt(std.mem.alignForward(usize, addr, @alignOf(pg.Conn)));
}

test "a second claim on the same connection is refused, a different one is not" {
    const a = fakeConn(0x10000);
    const b = fakeConn(0x20000);
    try mark(a, "vault_runtime");
    defer unmark(b);
    try testing.expectError(Error.ElevationRefused, mark(a, "billing_runtime"));
    try mark(b, "billing_runtime");
    unmark(a);
    try mark(a, "metering_runtime");
    unmark(a);
}

test "auditRelease clears the entry, counts the refusal, and is one-shot" {
    const c = fakeConn(0x30000);
    const before = refusedReleaseCount();
    try mark(c, "billing_runtime");
    const hit = auditRelease(c);
    try testing.expect(hit != null);
    try testing.expectEqualStrings("billing_runtime", hit.?);
    try testing.expectEqual(before + 1, refusedReleaseCount());
    // Cleared: a second audit of the same connection is the normal path.
    try testing.expect(auditRelease(c) == null);
    try testing.expectEqual(before + 1, refusedReleaseCount());
}

test "a full table refuses rather than growing" {
    var claimed: usize = 0;
    defer for (0..claimed) |i| unmark(fakeConn(0x40000 + i * 0x100));
    while (claimed < MAX_TRACKED_ELEVATIONS) : (claimed += 1) {
        mark(fakeConn(0x40000 + claimed * 0x100), "vault_runtime") catch break;
    }
    // Whatever the table already held, it is full now: the next distinct
    // connection is refused loudly instead of silently going unelevated.
    try testing.expectError(Error.ElevationRefused, mark(fakeConn(0x90000), "vault_runtime"));
}
