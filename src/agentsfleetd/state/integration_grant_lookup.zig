//! Read-only enforcement predicates over `core.integration_grants` — the ONLY
//! module that answers "may this fleet use this integration" (grant-gate
//! invariant: one grant-read implementation, imported by both enforcement
//! points: the mint handler and the lease classifier). The write side
//! (request/approve/revoke) lives in `http/handlers/integration_grants/`;
//! this module never writes. It also owns the status vocabulary so the http
//! layer imports state, never the reverse.
//!
//! RULE NSQ: schema-qualified SQL. RULE FLS: PgQuery with `defer q.deinit()`.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// `core.integration_grants.status` values — single source (RULE UFS). The
/// grants handler re-exports this for its writers and the approval webhook.
pub const GrantStatus = enum {
    pending,
    approved,
    revoked,

    pub fn toSlice(self: GrantStatus) []const u8 {
        return switch (self) {
            .pending => "pending",
            .approved => "approved",
            .revoked => "revoked",
        };
    }
};

const STATUS_APPROVED = GrantStatus.approved.toSlice();

/// One indexed read on the mint hot path: does `fleet_id` hold an APPROVED
/// grant for `service`? Absent, pending, and revoked rows are all false — the
/// caller fails closed and owns the wire mapping (`UZ-GRANT-001`).
pub fn isApproved(conn: *pg.Conn, fleet_id: []const u8, service: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT 1 FROM core.integration_grants
        \\WHERE fleet_id = $1::uuid AND service = $2 AND status = $3
        \\LIMIT 1
    , .{ fleet_id, service, STATUS_APPROVED }));
    defer q.deinit();
    return (try q.next()) != null;
}

/// The fleet's approved services in one batch read — called once per
/// lease-issue so the classification loop never queries per credential.
/// Slice + entries are owned by `alloc` (the lease path passes its request
/// arena, freeing wholesale; non-arena callers use `freeSet`).
pub fn approvedSet(alloc: std.mem.Allocator, conn: *pg.Conn, fleet_id: []const u8) ![]const []const u8 {
    var out: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (out.items) |s| alloc.free(s);
        out.deinit(alloc);
    }
    var q = PgQuery.from(try conn.query(
        \\SELECT service FROM core.integration_grants
        \\WHERE fleet_id = $1::uuid AND status = $2
    , .{ fleet_id, STATUS_APPROVED }));
    defer q.deinit();
    while (try q.next()) |row| {
        const dup = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(dup);
        try out.append(alloc, dup);
    }
    return out.toOwnedSlice(alloc);
}

/// Linear membership test over an `approvedSet` result — cardinality is
/// bounded by the supported-services count, so no map ceremony.
pub fn contains(set: []const []const u8, service: []const u8) bool {
    for (set) |s| if (std.mem.eql(u8, s, service)) return true;
    return false;
}

/// Release a non-arena `approvedSet` result.
pub fn freeSet(alloc: std.mem.Allocator, set: []const []const u8) void {
    for (set) |s| alloc.free(s);
    alloc.free(set);
}

// ── Tests ────────────────────────────────────────────────────────────────────
// The DB-backed predicates are proven in credentials_mint_integration_test.zig
// (live table, all three statuses); these pin the pure surface.

const testing = std.testing;

test "grant status vocabulary is the schema's exact strings" {
    try testing.expectEqualStrings("pending", GrantStatus.pending.toSlice());
    try testing.expectEqualStrings("approved", GrantStatus.approved.toSlice());
    try testing.expectEqualStrings("revoked", GrantStatus.revoked.toSlice());
}

test "contains: linear membership over an approved set" {
    const set = [_][]const u8{ "github", "slack" };
    try testing.expect(contains(set[0..], "github"));
    try testing.expect(!contains(set[0..], "zoho"));
    try testing.expect(!contains(&.{}, "github"));
}
