const std = @import("std");
const shared = @import("common");
const pg = @import("pg");
const protocol = @import("contract").protocol;
const db_fixtures = @import("../db/test_fixtures.zig");
const credential_key = @import("../fleet_runtime/credential_key.zig");
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
const STATIC_SENTINEL = "cp_static_sentinel";

fn seedFleetWithConfig(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, session_id: []const u8, config: []const u8) !void {
    try db_fixtures.seedFleet(conn, fleet_id, cp.WORKSPACE_ID, name, config, cp.SOURCE_MD);
    try db_fixtures.seedFleetSession(conn, session_id, fleet_id, "{}");
}

fn seedVaultJson(conn: *pg.Conn, name: []const u8, json: []const u8) !void {
    const key_name = try credential_key.allocKeyName(cp.ALLOC, name);
    defer cp.ALLOC.free(key_name);
    try vault.storeJsonPlaintext(cp.ALLOC, conn, cp.WORKSPACE_ID, key_name, json);
}

fn setGithubGrant(conn: *pg.Conn, fleet_id: []const u8, status: grant_lookup.GrantStatus) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason)
        \\VALUES ($1::uuid, $1, $2::uuid, $3, $4, 0, 'cp lease-gate test')
        \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status
    , .{ GRANT_CP_ID, fleet_id, PROVIDER_GITHUB, status.toSlice() });
}

fn leaseBodyAs(h: anytype, token: []const u8) ![]u8 {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
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
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_1_ID, "cp-gate-ungranted", cp.SESSION_1_ID, CONFIG_GITHUB_CRED);
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_2_ID, "cp-gate-granted", cp.SESSION_2_ID, CONFIG_GITHUB_CRED);
    try seedVaultJson(conn, PROVIDER_GITHUB, "{\"integration\":\"github\",\"installation_id\":\"42\"}");
    try setGithubGrant(conn, cp.AGENTSFLEET_2_ID, .approved);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_2_ID);

    var checked_ungranted = false;
    var checked_granted = false;
    for (0..2) |_| {
        const body = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
        defer cp.ALLOC.free(body);
        const parsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, body, .{});
        defer parsed.deinit();
        const lease = parsed.value.object.get("lease").?.object;
        const fleet_id = lease.get("event").?.object.get("fleet_id").?.string;
        const policy = lease.get("policy").?.object;
        const mintable = policy.get("mintable").?.array;
        const secrets_map = policy.get("secrets_map").?;
        if (std.mem.eql(u8, fleet_id, cp.AGENTSFLEET_1_ID)) {
            try std.testing.expectEqual(@as(usize, 0), mintable.items.len);
            if (secrets_map == .object) try std.testing.expect(secrets_map.object.get(PROVIDER_GITHUB) == null);
            try std.testing.expect(std.mem.indexOf(u8, body, "installation_id") == null);
            checked_ungranted = true;
        } else {
            try std.testing.expectEqual(@as(usize, 1), mintable.items.len);
            try std.testing.expectEqualStrings(PROVIDER_GITHUB, mintable.items[0].object.get("integration").?.string);
            checked_granted = true;
        }
    }
    try std.testing.expect(checked_ungranted);
    try std.testing.expect(checked_granted);
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
    try seedFleetWithConfig(conn, cp.AGENTSFLEET_1_ID, "cp-gate-static", cp.SESSION_1_ID, CONFIG_STATIC_CRED);
    try seedVaultJson(conn, "cpstatic", "{\"api_token\":\"" ++ STATIC_SENTINEL ++ "\"}");
    try cp.publishFreshEvent(h, cp.AGENTSFLEET_1_ID);

    const body = try leaseBodyAs(h, cp.RUNNER_A_TOKEN);
    defer cp.ALLOC.free(body);
    const parsed = try std.json.parseFromSlice(std.json.Value, cp.ALLOC, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease").?.object;
    const policy = lease.get("policy").?.object;
    try std.testing.expectEqual(@as(usize, 0), policy.get("mintable").?.array.items.len);
    const cpstatic = policy.get("secrets_map").?.object.get("cpstatic").?.object;
    try std.testing.expectEqualStrings(STATIC_SENTINEL, cpstatic.get("api_token").?.string);
}
