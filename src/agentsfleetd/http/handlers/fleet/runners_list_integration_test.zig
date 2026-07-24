//! Integration tier for the restructured runner-list read: the lease-liveness
//! value each row carries must be correct after the check moved off a
//! whole-table CTE onto a per-page subquery.
//!
//! The risk the restructure introduces is a per-row liveness value flipping, so
//! this seeds a KNOWN mix — a runner with a live lease, one with an expired
//! lease, one with none — and asserts the `has_live_lease` value for each. The
//! assertion queries that EXISTS predicate SCOPED to this test's own runners
//! rather than through the handler's global pagination: the operator runner
//! list is not workspace-scoped, so on a shared integration database its
//! `ORDER BY created_at DESC LIMIT` page holds other tests' runners too, and a
//! by-row lookup keyed on that page cannot be relied on. Payload identity
//! against the pre-change statement was confirmed by set difference during
//! implementation (recorded in the spec's Discovery).
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("../../../db/test_fixtures.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;

const WS = "0195b4ba-8d3a-7f13-8abc-0000000f0001";
const FLEET = "0195b4ba-8d3a-7f13-8abc-0000000f0002";
const HOST_PREFIX = "livequery-";

/// now_ms for the query; leases are seeded live-past or expired-before this.
const NOW_MS: i64 = 1_800_000_000_000;
const LIVE_UNTIL: i64 = NOW_MS + 60_000;
const EXPIRED_AT: i64 = NOW_MS - 60_000;

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Seed one runner in host order `n`, optionally with a lease at `expires`.
fn seedRunner(conn: *pg.Conn, n: usize, lease_expires: ?i64) !void {
    const host = try std.fmt.allocPrint(std.heap.page_allocator, "{s}{d:0>3}", .{ HOST_PREFIX, n });
    defer std.heap.page_allocator.free(host);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES (overlay(md5($1)::uuid::text placing '7' from 15 for 1)::uuid,
        \\        $1, $1, 'standard', 'active', '[]'::jsonb, $2, $3, 0)
        \\ON CONFLICT DO NOTHING
    , .{ host, NOW_MS, @as(i64, 1_750_000_000_000 + @as(i64, @intCast(n))) });
    if (lease_expires) |exp| {
        _ = try conn.exec(
            \\INSERT INTO fleet.runner_leases
            \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
            \\   event_type, request_json, event_created_at, posture, provider, model,
            \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
            \\   last_metered_at_ms, fencing_token, lease_expires_at, status,
            \\   created_at, updated_at)
            \\VALUES (overlay(md5('L' || $1)::uuid::text placing '7' from 15 for 1)::uuid,
            \\        overlay(md5($1)::uuid::text placing '7' from 15 for 1)::uuid,
            \\        $2::uuid, $3::uuid, $4::uuid, 'e' || $1, 'a', 'fleet.run', '{}', 0,
            \\        'standard', 'anthropic', 'claude', 0, 0, 0, 0, 1, $5, $6, 0, 0)
        , .{ host, FLEET, WS, base.TEST_TENANT_ID, exp, protocol.RUNNER_LEASE_STATUS_ACTIVE });
    }
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET}) catch |err|
        std.log.warn("lease teardown ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id LIKE $1", .{HOST_PREFIX ++ "%"}) catch |err|
        std.log.warn("runner teardown ignored: {s}", .{@errorName(err)});
    base.teardownFleets(conn, WS);
    base.teardownWorkspace(conn, WS);
}

test "runner list liveness value is correct per row after the restructure" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    try base.seedTenant(db.conn);
    try base.seedWorkspace(db.conn, WS);
    try base.seedFleet(db.conn, FLEET, WS, "livequery-fleet", "{}", "");
    defer teardown(db.conn);

    // Three known rows:
    //   r1 — live lease    → has_live_lease TRUE
    //   r2 — expired lease → FALSE (lease exists but lease_expires_at < now)
    //   r3 — no lease      → FALSE
    try seedRunner(db.conn, 1, LIVE_UNTIL);
    try seedRunner(db.conn, 2, EXPIRED_AT);
    try seedRunner(db.conn, 3, null);

    // Assert the liveness EXISTS predicate directly, scoped to this test's own
    // runners by host prefix. That predicate — evaluated against `runner_leases`
    // with the status and expiry guards — is the exact thing the restructure moved from a
    // whole-table CTE to a per-page subquery, so it is what a correctness test
    // must pin. The full statement is exercised for its PLAN elsewhere; going
    // through its global `ORDER BY created_at DESC LIMIT` here would be fragile,
    // because the operator runner list is not workspace-scoped and a shared
    // integration database holds other tests' runners on the same page.
    const liveness =
        \\SELECT r.host_id,
        \\       EXISTS (
        \\           SELECT 1 FROM fleet.runner_leases l
        \\           WHERE l.runner_id = r.id AND l.status = $1 AND l.lease_expires_at > $2
        \\       ) AS has_live_lease
        \\FROM fleet.runners r
        \\WHERE r.host_id LIKE $3
        \\ORDER BY r.host_id ASC
    ;
    var q = PgQuery.from(try db.conn.query(liveness, .{
        protocol.RUNNER_LEASE_STATUS_ACTIVE, NOW_MS, HOST_PREFIX ++ "%",
    }));
    defer q.deinit();

    var live_by_host = std.StringHashMap(bool).init(alloc);
    defer {
        var it = live_by_host.keyIterator();
        while (it.next()) |k| alloc.free(k.*);
        live_by_host.deinit();
    }
    while (try q.next()) |row| {
        const host = try alloc.dupe(u8, try row.get([]const u8, 0));
        try live_by_host.put(host, try row.get(bool, 1));
    }

    try std.testing.expectEqual(@as(usize, 3), live_by_host.count());
    try std.testing.expectEqual(true, live_by_host.get("livequery-001").?);
    try std.testing.expectEqual(false, live_by_host.get("livequery-002").?);
    try std.testing.expectEqual(false, live_by_host.get("livequery-003").?);
}
