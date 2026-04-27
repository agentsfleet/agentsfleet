/// test_fixtures_uc3.zig — shared tenant/workspace fixtures, post tenant_billing.
///
/// Usage per test:
///
///   try uc3.seed(conn, uc3.WS_UPGRADE);
///   defer uc3.teardown(conn, uc3.WS_UPGRADE);
const std = @import("std");
const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TEST_TENANT_ID = base.TEST_TENANT_ID;

// Segment 5 prefix cc01–cc0c for backward-compat consumers.
const WS_UPGRADE = "0195b4ba-8d3a-7f13-8abc-cc0000000001";
const WS_GRACE = "0195b4ba-8d3a-7f13-8abc-cc0000000002";
const WS_SYNC = "0195b4ba-8d3a-7f13-8abc-cc0000000003";
const WS_MISSING = "0195b4ba-8d3a-7f13-8abc-cc0000000004";
const WS_MANUAL_DOWNGRADE = "0195b4ba-8d3a-7f13-8abc-cc0000000005";
const WS_RESUBSCRIBE = "0195b4ba-8d3a-7f13-8abc-cc0000000006";
const WS_CASCADE = "0195b4ba-8d3a-7f13-8abc-cc0000000007";
const WS_LIMIT_BLOCK = "0195b4ba-8d3a-7f13-8abc-cc0000000008";
const WS_LIMIT_IGNORE = "0195b4ba-8d3a-7f13-8abc-cc0000000009";
const WS_EMPTY_SUB = "0195b4ba-8d3a-7f13-8abc-cc000000000a";
const WS_WHITESPACE_SUB = "0195b4ba-8d3a-7f13-8abc-cc000000000b";
const WS_EXCLUDE_SELF = "0195b4ba-8d3a-7f13-8abc-cc000000000c";

const TENANT_LIMIT_BLOCK = "0195b4ba-8d3a-7f13-8abc-cc0000000051";
const TENANT_LIMIT_IGNORE = "0195b4ba-8d3a-7f13-8abc-cc0000000052";
const TENANT_EXCLUDE_SELF = "0195b4ba-8d3a-7f13-8abc-cc0000000053";

pub fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
}

fn seedWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8, tenant_name: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenantById(conn, tenant_id, tenant_name);
    try base.seedWorkspaceWithTenant(conn, workspace_id, tenant_id);
}

pub fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}

fn teardownWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, tenant_id);
}
