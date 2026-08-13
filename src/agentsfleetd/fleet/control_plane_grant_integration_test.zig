const std = @import("std");
const shared = @import("common");
const pg = @import("pg");
const protocol = @import("contract").protocol;
const db_fixtures = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const vault = @import("../state/vault.zig");
const grant_lookup = @import("../state/integration_grant_lookup.zig");
const cp = @import("control_plane_integration_test.zig");

const PROVIDER_GITHUB = shared.PROVIDER_GITHUB;
const CONFIG_GITHUB_CRED =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["github"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_STATIC_CRED =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["cpstatic"],"budget":{"daily_dollars":5.0}}}
;
const GRANT_CP_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6f02";

/// The lease object, or a named error.
///
/// A bare `.?.object` on a null lease does not fail this test — it PANICS the
/// whole test binary, so every remaining `defer` is skipped, including this
/// file's `cp.cleanupAll`. The leaked fixtures then break unrelated suites:
/// `runner-cp-a` survives and collides on `uq_runners_token_hash`, and
/// `cp.WORKSPACE_ID` survives as the lowest uuid under the shared test tenant,
/// which is what `resolvePrimaryWorkspace` hands to `tenant_provider`. One
/// assertion failure became ~26. Returning an error keeps the blast radius to
/// this test and names what actually went wrong.
fn expectLease(value: std.json.Value) !std.json.ObjectMap {
    const lease = value.object.get("lease") orelse return error.LeaseFieldAbsent;
    if (lease == .null) return error.ExpectedLeaseGotNull;
    return lease.object;
}
const STATIC_SENTINEL = "cp_static_sentinel";

fn seedFleetWithConfig(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, config: []const u8) !void {
    try db_fixtures.seedFleet(conn, fleet_id, cp.WORKSPACE_ID, name, config, cp.SOURCE_MD);
    try db_fixtures.seedFleetSession(conn, fleet_id, "{}");
}

fn seedVaultJson(conn: *pg.Conn, name: []const u8, json: []const u8) !void {
    try vault.storeJsonPlaintext(cp.ALLOC, conn, cp.WORKSPACE_ID, name, json);
}

fn setGithubGrant(conn: *pg.Conn, fleet_id: []const u8, status: grant_lookup.GrantStatus) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (id, fleet_id, service, status, created_at, requested_reason)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, 0, 'cp lease-gate test')
        \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status
    , .{ GRANT_CP_ID, fleet_id, PROVIDER_GITHUB, status.toSlice() });
}

fn leaseBodyAs(h: anytype, token: []const u8) ![]u8 {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json(protocol.LEASE_REQUEST_CURRENT_JSON);
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return cp.ALLOC.dupe(u8, resp.body);
}

test "integration: test_lease_gates_mintable_on_grant" {
    crypto_primitives.setTestKek();
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_gate_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_2_ID, "cp-gate-granted", CONFIG_GITHUB_CRED);
    try seedVaultJson(conn, PROVIDER_GITHUB, "{\"integration\":\"github\",\"installation_id\":\"42\"}");
    try setGithubGrant(conn, cp.AGENTSFLEET_2_ID, .approved);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_2_ID);

    // Only the GRANTED fleet has work, so the poll below is deterministic: the
    // ungranted half is its own test now that an ungranted credential parks the
    // event instead of yielding a lease — two eligible fleets would make
    // which one the selector picks first decide the assertion.
    const body = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
    defer cp.ALLOC.free(body);
    const parsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, body, .{});
    defer parsed.deinit();
    const lease = try expectLease(parsed.value);
    try std.testing.expectEqualStrings(cp.AGENTSFLEET_2_ID, lease.get("event").?.object.get("fleet_id").?.string);
    const mintable = lease.get("policy").?.object.get("mintable").?.array;
    try std.testing.expectEqual(@as(usize, 1), mintable.items.len);
    try std.testing.expectEqualStrings(PROVIDER_GITHUB, mintable.items[0].object.get("integration").?.string);
}

test "integration: test_lease_parks_on_missing_grant" {
    // Dimension 8.3. The credential resolves to a mintable handle, the fleet
    // holds no approved grant, and the ONLY eligible event belongs to it.
    //
    // This once issued a lease with the mintable silently dropped from
    // both surfaces: the runner took work it could never mint for, the run
    // failed at the far end, and nothing recorded that a decision was owed.
    // Now the event parks — no lease — and stays leasable, so an approval takes
    // effect on the next poll with no redeploy.
    crypto_primitives.setTestKek();
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_park_key");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_1_ID, "cp-gate-ungranted", CONFIG_GITHUB_CRED);
    try seedVaultJson(conn, PROVIDER_GITHUB, "{\"integration\":\"github\",\"installation_id\":\"42\"}");
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);

    const body = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
    defer cp.ALLOC.free(body);
    const parsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, body, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value.object.get("lease").? == .null);

    // Parked, not consumed: nothing leaked the handle config on the way out,
    // and a second poll still finds the event rather than having dropped it.
    try std.testing.expect(std.mem.indexOf(u8, body, "installation_id") == null);
    const again = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
    defer cp.ALLOC.free(again);
    const reparsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, again, .{});
    defer reparsed.deinit();
    try std.testing.expect(reparsed.value.object.get("lease").? == .null);
}

test "integration: test_static_secrets_unaffected_by_grant_gate" {
    crypto_primitives.setTestKek();
    const h = try cp.startHarness(cp.ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cp.cleanupAll(h, conn);

    try db_fixtures.seedTenant(conn);
    try db_fixtures.seedWorkspace(conn, cp.WORKSPACE_ID);
    try db_fixtures.seedPlatformProviderWithKey(cp.ALLOC, conn, cp.WORKSPACE_ID, "fw_gate_key2");
    try cp.fundLargeBalance(conn);
    try cp.seedRunner(conn, cp.RUNNER_A_ID, "runner-cp-a", cp.RUNNER_A_TOKEN);
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_1_ID, "cp-gate-static", CONFIG_STATIC_CRED);
    try seedVaultJson(conn, "cpstatic", "{\"api_token\":\"" ++ STATIC_SENTINEL ++ "\"}");
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);

    const body = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
    defer cp.ALLOC.free(body);
    const parsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, body, .{});
    defer parsed.deinit();
    const lease = try expectLease(parsed.value);
    const policy = lease.get("policy").?.object;
    try std.testing.expectEqual(@as(usize, 0), policy.get("mintable").?.array.items.len);
    const cpstatic = policy.get("secrets_map").?.object.get("cpstatic").?.object;
    try std.testing.expectEqualStrings(STATIC_SENTINEL, cpstatic.get("api_token").?.string);
}
