const std = @import("std");
const clock = @import("common").clock;
const db_fixtures = @import("../db/test_fixtures.zig");
const affinity = @import("affinity.zig");
const cp = @import("control_plane_integration_test.zig");

test "integration: runner control plane — report with a stale fencing token is rejected, writes nothing" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.seedActiveLease(conn, cp.LEASE_OLD_ID, cp.RUNNER_A_ID, cp.AGENTSFLEET_1_ID, 1);
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 2, clock.nowMillis() + 60_000);

    const resp = try cp.reportLease(h, cp.RUNNER_A_TOKEN, cp.LEASE_OLD_ID, 1);
    defer resp.deinit();
    try resp.expectErrorCode("UZ-RUN-005");
    try std.testing.expect(try cp.leaseStatusIs(conn, cp.LEASE_OLD_ID, "active"));
}

test "integration: runner control plane — an expired lease is reclaimed and re-fenced with a higher token" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedRunner(conn, cp.RUNNER_B_ID, "runner-cp-b", cp.RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 1, 0);
    try cp.seedActiveLease(conn, cp.LEASE_OLD_ID, cp.RUNNER_A_ID, cp.AGENTSFLEET_1_ID, 1);

    const lv = try cp.leaseAs(h, cp.RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| cp.ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(cp.AGENTSFLEET_1_ID, lv.fleet_id.?);
    try std.testing.expect(lv.fencing_token > 1);
    try std.testing.expect(try cp.leaseStatusIs(conn, cp.LEASE_OLD_ID, "expired"));

    const rep = try cp.reportLease(h, cp.RUNNER_A_TOKEN, cp.LEASE_OLD_ID, 1);
    defer rep.deinit();
    try rep.expectErrorCode("UZ-RUN-005");
}

test "integration: runner control plane — a fresh lease carries the resolved provider key on the policy" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    const KNOWN_KEY = "fw_lease_path_known_key";
    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, KNOWN_KEY);
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);
    try cp.expectLeasePolicyKey(h, cp.RUNNER_A_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a reclaimed lease re-resolves and carries the provider key" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    const KNOWN_KEY = "fw_reclaim_path_known_key";
    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, KNOWN_KEY);
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedRunner(conn, cp.RUNNER_B_ID, "runner-cp-b", cp.RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 1, 0);
    try cp.seedActiveLease(conn, cp.LEASE_OLD_ID, cp.RUNNER_A_ID, cp.AGENTSFLEET_1_ID, 1);
    try cp.expectLeasePolicyKey(h, cp.RUNNER_B_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a fresh lease overlays the resolved context cap+model onto sentinel frontmatter" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

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
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);
    try cp.expectLeasePolicyContext(h, cp.RUNNER_A_TOKEN, OVERLAY_CAP_TOKENS, 30, OVERLAY_MODEL);
}

test "integration: runner control plane — a fresh lease carries the installed SKILL.md instructions" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_instr_fresh_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);
    try cp.expectLeaseInstructions(h, cp.RUNNER_A_TOKEN, "You are a control-plane test fleet.");
}

test "integration: runner control plane — a reclaimed lease keeps the installed SKILL.md instructions" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_instr_reclaim_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedRunner(conn, cp.RUNNER_B_ID, "runner-cp-b", cp.RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 1, 0);
    try cp.seedActiveLease(conn, cp.LEASE_OLD_ID, cp.RUNNER_A_ID, cp.AGENTSFLEET_1_ID, 1);
    try cp.expectLeaseInstructions(h, cp.RUNNER_B_TOKEN, "You are a control-plane test fleet.");
}

test "integration: runner control plane — sticky routing is a hint, not ownership" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedRunner(conn, cp.RUNNER_B_ID, "runner-cp-b", cp.RUNNER_B_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 1, 0);
    try cp.seedActiveLease(conn, cp.LEASE_OLD_ID, cp.RUNNER_A_ID, cp.AGENTSFLEET_1_ID, 1);

    const lv = try cp.leaseAs(h, cp.RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| cp.ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(cp.AGENTSFLEET_1_ID, lv.fleet_id.?);
    try std.testing.expect(try cp.activeLeaseRunnerIs(conn, cp.AGENTSFLEET_1_ID, cp.RUNNER_B_ID));
}

test "integration: runner control plane — release is token-guarded: a superseded holder cannot free the live slot" {
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try cp.seedActiveFleet(conn, cp.AGENTSFLEET_1_ID, "cp-fleet-1", cp.SESSION_1_ID);
    const live_until = clock.nowMillis() + 60_000;
    try cp.seedAffinity(conn, cp.AFFINITY_1_ID, cp.AGENTSFLEET_1_ID, cp.RUNNER_A_ID, 2, live_until);

    try affinity.release(conn, cp.AGENTSFLEET_1_ID, 1);
    try std.testing.expectEqual(live_until, try cp.leasedUntilOf(conn, cp.AGENTSFLEET_1_ID));
    try affinity.release(conn, cp.AGENTSFLEET_1_ID, 2);
    try std.testing.expect(try cp.leasedUntilOf(conn, cp.AGENTSFLEET_1_ID) < live_until);
}
