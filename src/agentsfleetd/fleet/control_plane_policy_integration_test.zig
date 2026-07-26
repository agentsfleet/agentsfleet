// Runner control-plane policy proofs: fencing at report, expiry-reclaim, the
// lease-time provider/context/instruction overlay, and sticky-as-a-hint.
//
// Rides `control_plane_integration_test.zig`'s harness and seed helpers but owns
// every ROW it writes. Sharing that file's ids coupled the two suites to each
// other's teardown — including on `fleet.runners.token_hash`, whose UNIQUE
// constraint `ON CONFLICT (id) DO NOTHING` does not guard, so a leaked runner
// row from one file made the other's seed fail on a constraint it never names.
//
// Reclaim fixtures publish a real event before they write their stranded lease.
// Discovery goes through the readiness index now, and in production a fleet
// holding an `active` lease keeps its mark for free: the poll-site clear is
// reachable only after `reclaimPriorActive` returns null, which cannot happen
// while that lease exists. A lease row written with raw SQL never travels the
// append that marked its fleet, so the fleet is invisible to every poll and the
// reclaim under test can never run. The mark therefore has to arrive the way
// production makes it — through the one producer. Writing the index field by
// hand would be the shortcut, and would also stop these suites noticing a
// producer that stopped marking at all.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const db_fixtures = @import("../db/test_fixtures.zig");
const affinity = @import("affinity.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const TestHarness = @import("../http/test_harness.zig").TestHarness;
const cp = @import("control_plane_integration_test.zig");

// Node suffix `…0d5…` belongs to this suite alone.
const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5a01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5b01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5c01";
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5d01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5e01";
const LEASE_OLD_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d5f01";

// Distinct token bodies for the same reason as the ids — a shared body is a
// shared `token_hash`. The node-suffixed shape mirrors `placement_eligibility`.
const RUNNER_A_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "2b3e1e0d" ++ "a" ** 56;
const RUNNER_B_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "2b3e1e0d" ++ "b" ** 56;

const INSTRUCTIONS_SUBSTR = "You are a control-plane test fleet.";

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

/// Idempotent teardown of every row this suite seeds, plus the fleet's Redis
/// footprint. `purgeFleetRedisState` is what stops a mark outliving the fleet
/// row: `fleet:ready` is ONE key shared by every suite in the binary and `peek`
/// reads a bounded random sample of it, so a leftover field costs a sibling a
/// slot of that sample.
fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    redis_fleet.purgeFleetRedisState(&h.queue, FLEET_ID) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{cp.WORKSPACE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID});
    db_fixtures.teardownPlatformProvider(conn, cp.WORKSPACE_ID);
    db_fixtures.teardownFleets(conn, cp.WORKSPACE_ID);
    db_fixtures.teardownWorkspace(conn, cp.WORKSPACE_ID);
    db_fixtures.teardownTenant(conn);
}

/// The state production leaves behind when a runner dies holding a lease: the
/// fleet marked ready by a real append, its claim lapsed, and its lease row
/// still `active`. See the module note for why the append is load-bearing.
fn seedStrandedLease(h: *TestHarness, conn: *pg.Conn, fencing_token: i64) !void {
    try cp.publishFreshEvent(h, FLEET_ID);
    try cp.seedAffinity(conn, AFFINITY_ID, FLEET_ID, RUNNER_A_ID, fencing_token, 0);
    try cp.seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, FLEET_ID, fencing_token);
}

test "integration: runner control plane — report with a stale fencing token is rejected, writes nothing" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try cp.seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, FLEET_ID, 1);
    try cp.seedAffinity(conn, AFFINITY_ID, FLEET_ID, RUNNER_A_ID, 2, clock.nowMillis() + 60_000);

    const resp = try cp.reportLease(h, RUNNER_A_TOKEN, LEASE_OLD_ID, 1);
    defer resp.deinit();
    try resp.expectErrorCode("UZ-RUN-005");
    try std.testing.expect(try cp.leaseStatusIs(conn, LEASE_OLD_ID, "active"));
}

test "integration: runner control plane — an expired lease is reclaimed and re-fenced with a higher token" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedRunner(conn, RUNNER_B_ID, "runner-policy-b", RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try seedStrandedLease(h, conn, 1);

    const lv = try cp.leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| cp.ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(FLEET_ID, lv.fleet_id.?);
    try std.testing.expect(lv.fencing_token > 1);
    try std.testing.expect(try cp.leaseStatusIs(conn, LEASE_OLD_ID, "expired"));

    const rep = try cp.reportLease(h, RUNNER_A_TOKEN, LEASE_OLD_ID, 1);
    defer rep.deinit();
    try rep.expectErrorCode("UZ-RUN-005");
}

test "integration: runner control plane — a fresh lease carries the resolved provider key on the policy" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    const KNOWN_KEY = "fw_lease_path_known_key";
    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, KNOWN_KEY);
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try cp.publishFreshEvent(h, FLEET_ID);
    try cp.expectLeasePolicyKey(h, RUNNER_A_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a reclaimed lease re-resolves and carries the provider key" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    const KNOWN_KEY = "fw_reclaim_path_known_key";
    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, KNOWN_KEY);
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedRunner(conn, RUNNER_B_ID, "runner-policy-b", RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try seedStrandedLease(h, conn, 1);
    try cp.expectLeasePolicyKey(h, RUNNER_B_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a fresh lease overlays the resolved context cap+model onto sentinel frontmatter" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    const OVERLAY_MODEL = "accounts/fireworks/models/kimi-k2.6";
    const OVERLAY_CAP_TOKENS = 1_000_000;
    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_overlay_path_key");
    _ = try conn.exec(
        "UPDATE core.platform_provider_defaults SET context_cap_tokens = $1 WHERE active = true",
        .{@as(i32, OVERLAY_CAP_TOKENS)},
    );
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try cp.publishFreshEvent(h, FLEET_ID);
    try cp.expectLeasePolicyContext(h, RUNNER_A_TOKEN, OVERLAY_CAP_TOKENS, 30, OVERLAY_MODEL);
}

test "integration: runner control plane — a fresh lease carries the installed SKILL.md instructions" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_instr_fresh_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try cp.publishFreshEvent(h, FLEET_ID);
    try cp.expectLeaseInstructions(h, RUNNER_A_TOKEN, INSTRUCTIONS_SUBSTR);
}

test "integration: runner control plane — a reclaimed lease keeps the installed SKILL.md instructions" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_instr_reclaim_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedRunner(conn, RUNNER_B_ID, "runner-policy-b", RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try seedStrandedLease(h, conn, 1);
    try cp.expectLeaseInstructions(h, RUNNER_B_TOKEN, INSTRUCTIONS_SUBSTR);
}

test "integration: runner control plane — sticky routing is a hint, not ownership" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedRunner(conn, RUNNER_B_ID, "runner-policy-b", RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    try seedStrandedLease(h, conn, 1);

    const lv = try cp.leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| cp.ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(FLEET_ID, lv.fleet_id.?);
    try std.testing.expect(try cp.activeLeaseRunnerIs(conn, FLEET_ID, RUNNER_B_ID));
}

test "integration: runner control plane — release is token-guarded: a superseded holder cannot free the live slot" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, RUNNER_A_ID, "runner-policy-a", RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, FLEET_ID, "policy-fleet-1", SESSION_ID);
    const live_until = clock.nowMillis() + 60_000;
    try cp.seedAffinity(conn, AFFINITY_ID, FLEET_ID, RUNNER_A_ID, 2, live_until);

    try affinity.release(conn, FLEET_ID, 1);
    try std.testing.expectEqual(live_until, try cp.leasedUntilOf(conn, FLEET_ID));
    try affinity.release(conn, FLEET_ID, 2);
    try std.testing.expect(try cp.leasedUntilOf(conn, FLEET_ID) < live_until);
}
