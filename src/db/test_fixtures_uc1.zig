/// test_fixtures_uc1.zig — UC1: Free-plan credit exhaustion integration fixtures.
/// Tenant: scrooge-mcduck (the money guy). Workspaces named after Goofy's operations.
///
/// Covers: src/state/workspace_credit.zig integration tests
/// Tables seeded: tenants, workspaces
/// Tables cleaned via CASCADE: workspace_credit_state, workspace_credit_audit
///
/// Each test gets its own workspace UUID so tests are fully isolated even
/// when run in the same DB session.
///
/// Usage per test:
///
///   try uc1.seed(conn, uc1.WS_PROVISION);
///   defer uc1.teardown(conn, uc1.WS_PROVISION);
const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TEST_TENANT_ID = base.TEST_TENANT_ID;

// One workspace UUID per test in workspace_credit.zig.
// Segment 5 (aa01–aa04) identifies UC1 workspaces; easy to grep and clean.
pub const WS_PROVISION = "0195b4ba-8d3a-7f13-8abc-aa0000000001";
pub const WS_ENFORCE = "0195b4ba-8d3a-7f13-8abc-aa0000000002";
pub const WS_DEDUCT = "0195b4ba-8d3a-7f13-8abc-aa0000000003";
pub const WS_CLAMP = "0195b4ba-8d3a-7f13-8abc-aa0000000004";

/// Seed tenant + workspace. Idempotent via ON CONFLICT DO NOTHING.
pub fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
}

/// Delete workspace (cascades credit state + audit rows) then tenant.
pub fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}
