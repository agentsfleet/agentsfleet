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
pub const WS_UPGRADE = "0195b4ba-8d3a-7f13-8abc-cc0000000001";
pub const WS_GRACE = "0195b4ba-8d3a-7f13-8abc-cc0000000002";
pub const WS_SYNC = "0195b4ba-8d3a-7f13-8abc-cc0000000003";
pub const WS_MISSING = "0195b4ba-8d3a-7f13-8abc-cc0000000004";
pub const WS_MANUAL_DOWNGRADE = "0195b4ba-8d3a-7f13-8abc-cc0000000005";
pub const WS_RESUBSCRIBE = "0195b4ba-8d3a-7f13-8abc-cc0000000006";
pub const WS_CASCADE = "0195b4ba-8d3a-7f13-8abc-cc0000000007";
pub const WS_LIMIT_BLOCK = "0195b4ba-8d3a-7f13-8abc-cc0000000008";
pub const WS_LIMIT_IGNORE = "0195b4ba-8d3a-7f13-8abc-cc0000000009";
pub const WS_EMPTY_SUB = "0195b4ba-8d3a-7f13-8abc-cc000000000a";
pub const WS_WHITESPACE_SUB = "0195b4ba-8d3a-7f13-8abc-cc000000000b";
pub const WS_EXCLUDE_SELF = "0195b4ba-8d3a-7f13-8abc-cc000000000c";

pub const TENANT_LIMIT_BLOCK = "0195b4ba-8d3a-7f13-8abc-cc0000000051";
pub const TENANT_LIMIT_IGNORE = "0195b4ba-8d3a-7f13-8abc-cc0000000052";
pub const TENANT_EXCLUDE_SELF = "0195b4ba-8d3a-7f13-8abc-cc0000000053";

pub fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
}

pub fn seedWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8, tenant_name: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenantById(conn, tenant_id, tenant_name);
    try base.seedWorkspaceWithTenant(conn, workspace_id, tenant_id);
}

pub fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}

pub fn teardownWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, tenant_id);
}
