//! The registry of connections currently inside an elevation scope.
//!
//! Split from `pool_elevation.zig` on a real seam: this module owns the table
//! and nothing else, and it stores role NAMES rather than the `Role` enum. That
//! keeps the import one-way — `pool_elevation` depends on this, never the
//! reverse — and it keeps a module that must never dereference a possibly-dead
//! connection free of anything that could.
//!
//! Entries are compared by POINTER IDENTITY only. A tracked connection may
//! already have been destroyed by the pool, so nothing here reads through the
//! pointer. Tests exploit this by marking fabricated addresses.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
pub const Error = error{
    /// The connection is already elevated (nesting is refused, RULE OWN), or
    /// the tracking table is full.
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

const Entry = struct { conn: *pg.Conn, role_name: []const u8 };

// One mutex, protecting exactly `g_elevated`.
var g_mutex: common.Mutex = .{};
var g_elevated: [MAX_TRACKED_ELEVATIONS]?Entry = [_]?Entry{null} ** MAX_TRACKED_ELEVATIONS;
var g_refused_marks = std.atomic.Value(u64).init(0);

/// Claim a slot for `conn`. Refuses a second claim on the same connection
/// (RULE OWN: one scope owns the elevation, never stacked) and refuses when
/// the table is full.
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
    const slot = free_slot orelse {
        // Table pressure, not misuse — counted separately from the nesting
        // refusal above so an operator can tell the two apart.
        _ = g_refused_marks.fetchAdd(1, .monotonic);
        return Error.ElevationRefused;
    };
    g_elevated[slot] = .{ .conn = conn, .role_name = role_name };
}

/// Release the slot. Safe to call for a connection that holds none.
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

/// Operator-facing count of elevations refused because the tracking table was
/// full. Non-zero means the table is undersized for the deployment's pool, so
/// the pressure is visible instead of arriving as unexplained 500s.
pub fn refusedMarkCount() u64 {
    return g_refused_marks.load(.monotonic);
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

const ROLE_A = "role_a";
const ROLE_B = "role_b";

/// Registry entries are compared by pointer identity and never dereferenced,
/// so an aligned fabricated address stands in for a connection.
fn fakeConn(addr: usize) *pg.Conn {
    return @ptrFromInt(std.mem.alignForward(usize, addr, @alignOf(pg.Conn)));
}

test "a second claim on the same connection is refused, a different one is not" {
    const a = fakeConn(0x10000);
    const b = fakeConn(0x20000);
    try mark(a, ROLE_A);
    defer unmark(b);
    try testing.expectError(Error.ElevationRefused, mark(a, ROLE_B));
    try mark(b, ROLE_B);
    unmark(a);
    try mark(a, ROLE_A);
    unmark(a);
}

test "a full table refuses and counts the pressure rather than failing silently" {
    // The path that had no test while this lived in an over-cap file: filling
    // every slot must refuse LOUDLY and move refusedMarkCount, because the
    // alternative an operator sees is an unexplained 500 with no signal that
    // the table — not the query — was the limit.
    const before_marks = refusedMarkCount();
    const base: usize = 0x100000;

    var filled: usize = 0;
    while (filled < MAX_TRACKED_ELEVATIONS) : (filled += 1) {
        mark(fakeConn(base + filled * @alignOf(pg.Conn)), ROLE_A) catch break;
    }
    defer {
        var i: usize = 0;
        while (i < filled) : (i += 1) unmark(fakeConn(base + i * @alignOf(pg.Conn)));
    }
    try testing.expectEqual(@as(usize, MAX_TRACKED_ELEVATIONS), filled);

    // One more connection than the table holds: refused, and counted.
    const overflow = fakeConn(base + MAX_TRACKED_ELEVATIONS * @alignOf(pg.Conn));
    try testing.expectError(Error.ElevationRefused, mark(overflow, ROLE_A));
    try testing.expectEqual(before_marks + 1, refusedMarkCount());

    // A freed slot is reusable — exhaustion is pressure, not a permanent wedge.
    unmark(fakeConn(base));
    try mark(overflow, ROLE_A);
    unmark(overflow);
    try mark(fakeConn(base), ROLE_A);
}

test "a nesting refusal does not move the table-pressure counter" {
    // The two refusals share an error but mean different things: one is caller
    // misuse, the other is capacity. An operator paging on pressure must not
    // be woken by misuse.
    const c = fakeConn(0x50000);
    const before = refusedMarkCount();
    try mark(c, ROLE_A);
    defer unmark(c);
    try testing.expectError(Error.ElevationRefused, mark(c, ROLE_B));
    try testing.expectEqual(before, refusedMarkCount());
}
