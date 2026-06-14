// Boundary-condition tests for the renewal + reclaim SQL core. Drives
// `renewal.renew` / `affinity.claim` / `reclaim.reclaimPriorActive` directly
// against seeded fleet rows on a live test-DB connection, passing an explicit
// `now_ms` so every clamp lands on an exact integer and the assertions are
// deterministic. Covers the three off-by-one edges the dual-row extend must get
// right:
//   - now_ms == lease_expires_at: a lease expiring *this instant* still renews
//     (the guard is `capped > now`, and `now + TTL > now`), advancing by TTL.
//   - created_at + MAX is one ms ahead of now: the renewal succeeds but clamps
//     to the hard cap, never past it — a wedged-but-emitting agent terminates.
//   - a zombie with an expired affinity slot but NO prior runner_leases row:
//     a second runner claims it fresh; reclaimPriorActive finds nothing to
//     re-lease (the zombie is simply free), so no stale row is resurrected.
//
// Requires LIVE_DB=1. Skipped (error.SkipZigTest) when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const constants = @import("common");
const renewal = @import("renewal.zig");
const affinity = @import("affinity.zig");
const reclaim = @import("reclaim.zig");

const ALLOC = std.testing.allocator;

const auth_mw = @import("../auth/middleware/mod.zig");
const MS_PER_SECOND = 1_000;

// These tests drive the SQL core directly against the DB, not through the HTTP
// middleware chain, so no policy wiring is needed — the harness just gives us a
// live pool. A no-op registry config satisfies the required field.
fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0da011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0daa01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dab01";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dac01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dae01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0daf01";
const EVENT_ID = "evt-edge-1"; // matches seedLease; metering rows keyed by it

// A fixed "now" for deterministic clamping. Lease created_at is set relative to
// this per-test so the cap is exercised precisely.
const NOW_MS: i64 = 1_900_000_000_000;

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $2 || '-th', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id });
}

fn seedAffinity(conn: *pg.Conn, last_runner_id: []const u8, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, ZOMBIE_ID, last_runner_id, fencing_seq, leased_until });
}

fn seedLease(conn: *pg.Conn, fencing_token: i64, created_at: i64, lease_expires_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-edge-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', $8, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, ZOMBIE_ID, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, lease_expires_at, created_at });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    // Breakdown + ledger rows are keyed by event_id and survive a lease/affinity
    // delete; clear them so a re-seeded case starts the per-event slice counter
    // clean (the affinity counter resets, so a leftover slice_seq would collide).
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM core.zombie_execution_telemetry WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_ID, RUNNER_B_ID });
    base.teardownTenant(conn);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

/// Read both rows' deadlines so a test can assert they moved together. Each
/// query is fully consumed + deinit'd in its own scope BEFORE the next is
/// issued — a `pg.Conn` allows only one open result at a time.
fn readDeadlines(conn: *pg.Conn) !struct { lease: i64, affinity: i64 } {
    const lease_until = try readBigint(conn, "SELECT lease_expires_at FROM fleet.runner_leases WHERE id = $1::uuid", LEASE_ID);
    const aff_until = try readBigint(conn, "SELECT leased_until FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", ZOMBIE_ID);
    return .{ .lease = lease_until, .affinity = aff_until };
}

fn readBigint(conn: *pg.Conn, sql: []const u8, id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
}

test "renew at the exact deadline boundary still extends by a full TTL" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_ID, "edge-host");
    // Both rows expire at EXACTLY now — the boundary case. Fence holds (token ==
    // seq); created_at recent so the cap is far away and TTL wins.
    try seedAffinity(conn, RUNNER_ID, 5, NOW_MS);
    try seedLease(conn, 5, NOW_MS - 2_000, NOW_MS);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});

    const want = NOW_MS + constants.LEASE_TTL_MS;
    try std.testing.expectEqual(renewal.RenewOutcome{ .renewed = want }, outcome);
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(want, after.lease);
    try std.testing.expectEqual(want, after.affinity); // both rows lock to the new deadline
}

test "renew one millisecond before the hard cap clamps to the exact cap" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_ID, "edge-host");
    // created_at is set so the cap (created_at + MAX) lands exactly one ms past
    // now: the renewal is still legal (capped > now) but clamps to the cap, not
    // the full TTL increment.
    const created = NOW_MS + 1 - constants.MAX_RUNTIME_MS;
    try seedAffinity(conn, RUNNER_ID, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(conn, 5, created, NOW_MS - MS_PER_SECOND);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});
    const want_cap = created + constants.MAX_RUNTIME_MS; // == NOW_MS + 1
    try std.testing.expectEqual(renewal.RenewOutcome{ .renewed = want_cap }, outcome);
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(want_cap, after.lease);
    try std.testing.expectEqual(want_cap, after.affinity); // rows lock to the exact boundary
}

test "a free zombie with an expired slot but no prior lease is claimed fresh" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_ID, "edge-host-a"); // prior holder, now gone
    try seedRunner(conn, RUNNER_B_ID, "edge-host-b"); // second runner claiming
    // Expired affinity (leased_until in the past), but NO runner_leases row was
    // ever seeded — the prior holder left a stale slot, nothing to reclaim.
    try seedAffinity(conn, RUNNER_ID, 3, 0);
    defer teardown(conn);

    // Second runner claims the now-free slot: wins under a strictly higher token.
    const claim = try affinity.claim(conn, ALLOC, ZOMBIE_ID, RUNNER_B_ID, constants.LEASE_TTL_MS);
    try std.testing.expect(claim == .won);
    try std.testing.expect(claim.won.token > 3); // monotonic bump past the stale seq

    // No prior active lease exists → reclaimPriorActive finds nothing to
    // re-lease (the zombie was simply free); no stale row resurrected.
    const prior = try reclaim.reclaimPriorActive(conn, ALLOC, ZOMBIE_ID);
    try std.testing.expect(prior == null);
}
