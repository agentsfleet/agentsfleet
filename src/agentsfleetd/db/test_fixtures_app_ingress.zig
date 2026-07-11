//! Real-schema fixtures for GitHub App ingress integration tests.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const base = @import("test_fixtures.zig");
const vault = @import("../state/vault.zig");

pub const TENANT_ID = "0195c102-5000-7000-8000-f00000000001";
pub const ADMIN_WORKSPACE_ID = "0195c102-5001-7000-8000-000000000001";
pub const WORKSPACE_ID = "0195c102-5002-7000-8000-000000000002";
pub const INSTALLATION_ID = "10250042";
pub const FLEET_PULL_ONE = "0195c102-5010-7000-8000-000000000010";
pub const FLEET_PULL_TWO = "0195c102-5011-7000-8000-000000000011";
pub const FLEET_WORKFLOW = "0195c102-5012-7000-8000-000000000012";
pub const FLEET_WRONG_REPO = "0195c102-5013-7000-8000-000000000013";
pub const FLEET_NO_REPOS = "0195c102-5014-7000-8000-000000000014";
pub const FLEET_NO_GRANT = "0195c102-5015-7000-8000-000000000015";

const PROVIDER = "github";
const REPOSITORY = "agentsfleet/agentsfleet";
const STATUS_ACTIVE = "active";
const STATUS_APPROVED = "approved";
const APP_KEY = "github-app";
const INSTALL_UID = "0195c102-5020-7000-8000-000000000020";

const CONFIG_PULL =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["pull_request"],"repositories":["agentsfleet/agentsfleet"]}],"tools":[],"budget":{"daily_dollars":1}}}
;
const CONFIG_WORKFLOW =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["workflow_run"],"repositories":["agentsfleet/agentsfleet"]}],"tools":[],"budget":{"daily_dollars":1}}}
;
const CONFIG_WRONG_REPO =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["pull_request"],"repositories":["agentsfleet/docs"]}],"tools":[],"budget":{"daily_dollars":1}}}
;
const CONFIG_NO_REPOS =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["pull_request"]}],"tools":[],"budget":{"daily_dollars":1}}}
;

/// Seed the platform App bag, installation map, fleets, and approved grants.
pub fn seed(alloc: std.mem.Allocator, conn: *pg.Conn, webhook_secret: []const u8) !void {
    try base.seedTenantById(conn, TENANT_ID, "github-app-ingress-suite");
    try base.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try seedAppBag(alloc, conn, webhook_secret);
    const now = clock.nowMillis();
    const scopes: []const []const u8 = &.{};
    _ = try conn.exec(
        \\INSERT INTO core.connector_installs
        \\  (uid, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7)
        \\ON CONFLICT (provider, external_account_id) DO UPDATE SET workspace_id = EXCLUDED.workspace_id, updated_at = EXCLUDED.updated_at
    , .{ INSTALL_UID, PROVIDER, INSTALLATION_ID, WORKSPACE_ID, "", scopes, now });
    try seedFleet(conn, FLEET_PULL_ONE, "app-pr-one", CONFIG_PULL, now);
    try seedFleet(conn, FLEET_PULL_TWO, "app-pr-two", CONFIG_PULL, now);
    try seedFleet(conn, FLEET_WORKFLOW, "app-workflow", CONFIG_WORKFLOW, now);
    try seedFleet(conn, FLEET_WRONG_REPO, "app-wrong-repo", CONFIG_WRONG_REPO, now);
    try seedFleet(conn, FLEET_NO_REPOS, "app-no-repos", CONFIG_NO_REPOS, now);
    try seedFleet(conn, FLEET_NO_GRANT, "app-no-grant", CONFIG_PULL, now);
    try seedGrant(conn, FLEET_PULL_ONE, "0195c102-5030-7000-8000-000000000030", now);
    try seedGrant(conn, FLEET_PULL_TWO, "0195c102-5031-7000-8000-000000000031", now);
    try seedGrant(conn, FLEET_WORKFLOW, "0195c102-5032-7000-8000-000000000032", now);
    try seedGrant(conn, FLEET_WRONG_REPO, "0195c102-5033-7000-8000-000000000033", now);
    try seedGrant(conn, FLEET_NO_REPOS, "0195c102-5034-7000-8000-000000000034", now);
}

fn seedAppBag(alloc: std.mem.Allocator, conn: *pg.Conn, webhook_secret: []const u8) !void {
    var bag: std.json.ObjectMap = .empty;
    defer bag.deinit(alloc);
    try bag.put(alloc, "app_id", .{ .string = "102500" });
    try bag.put(alloc, "app_slug", .{ .string = "agentsfleet-test" });
    try bag.put(alloc, "private_key_pem", .{ .string = "test-private-key" });
    try bag.put(alloc, "webhook_secret", .{ .string = webhook_secret });
    try base.storeVaultJson(alloc, conn, ADMIN_WORKSPACE_ID, APP_KEY, .{ .object = bag });
}

fn seedFleet(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, config: []const u8, now: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5::jsonb, $6, $7, $7)
        \\ON CONFLICT (id) DO UPDATE SET config_json = EXCLUDED.config_json, status = EXCLUDED.status, updated_at = EXCLUDED.updated_at
    , .{ fleet_id, WORKSPACE_ID, name, "# test fleet", config, STATUS_ACTIVE, now });
}

fn seedGrant(conn: *pg.Conn, fleet_id: []const u8, grant_id: []const u8, now: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason, approved_at)
        \\VALUES ($1::uuid, $1, $2::uuid, $3, $4, $5, $6, $5)
        \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status, approved_at = EXCLUDED.approved_at
    , .{ grant_id, fleet_id, PROVIDER, STATUS_APPROVED, now, "App ingress integration test" });
}

/// Remove every seeded row in foreign-key order.
pub fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid AND name LIKE 'app-fanout-%'", .{WORKSPACE_ID}) catch |err| std.log.warn("App ingress fanout cleanup ignored: {s}", .{@errorName(err)});
    const fleets: []const []const u8 = &.{ FLEET_PULL_ONE, FLEET_PULL_TWO, FLEET_WORKFLOW, FLEET_WRONG_REPO, FLEET_NO_REPOS, FLEET_NO_GRANT };
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = ANY($1::uuid[])", .{fleets}) catch |err| std.log.warn("App ingress event cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = ANY($1::uuid[])", .{fleets}) catch |err| std.log.warn("App ingress fleet cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ PROVIDER, INSTALLATION_ID }) catch |err| std.log.warn("App ingress install cleanup ignored: {s}", .{@errorName(err)});
    _ = vault.deleteCredential(conn, ADMIN_WORKSPACE_ID, APP_KEY) catch |err| std.log.warn("App ingress vault cleanup ignored: {s}", .{@errorName(err)});
}

test "App ingress fixture pins the routed repository" {
    try std.testing.expectEqualStrings("agentsfleet/agentsfleet", REPOSITORY);
}
