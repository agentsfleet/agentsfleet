/// test_fixtures_uc2.zig — UC2: Pipeline proposal test fixtures.
///
/// Covers all pipeline proposal, guard, idempotency, lifecycle, scoring,
/// reporting, and architecture integration tests.
///
/// Agent IDs are UUIDs with prefix 0195b4ba-8d3a-7f13-8abc-dd<seq>.
/// Workspace IDs are in the cc02-cc04 range to avoid conflicts with UC3.
///
/// Usage per test:
///
///   try base.seedTenant(conn);
///   try base.seedWorkspace(conn, uc2.WS_E2E_1);
///   defer uc2.teardownWorkspace(conn, uc2.WS_E2E_1);
///   defer base.teardownTenant(conn);
const std = @import("std");
const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TEST_TENANT_ID = base.TEST_TENANT_ID;

// ── Agent UUID constants ─────────────────────────────────────────────────────
// Sequential assignment: dd0000000001 .. dd000000002e (46 total)

pub const AGENT_CTX_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000001";
pub const AGENT_CTX_2 = "0195b4ba-8d3a-7f13-8abc-dd0000000002";
pub const AGENT_CTX_3 = "0195b4ba-8d3a-7f13-8abc-dd0000000003";

pub const AGENT_GUARD_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000004";
pub const AGENT_GUARD_2 = "0195b4ba-8d3a-7f13-8abc-dd0000000005";
pub const AGENT_GUARD_3 = "0195b4ba-8d3a-7f13-8abc-dd0000000006";

pub const AGENT_GUARD_EDGE_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000007";
pub const AGENT_GUARD_EDGE_2 = "0195b4ba-8d3a-7f13-8abc-dd0000000008";
pub const AGENT_GUARD_EDGE_3 = "0195b4ba-8d3a-7f13-8abc-dd0000000009";
pub const AGENT_GUARD_EDGE_4 = "0195b4ba-8d3a-7f13-8abc-dd000000000a";
pub const AGENT_GUARD_EDGE_5 = "0195b4ba-8d3a-7f13-8abc-dd000000000b";

pub const AGENT_IDEM_LIST_1 = "0195b4ba-8d3a-7f13-8abc-dd000000000c";
pub const AGENT_IDEM_REJ_1 = "0195b4ba-8d3a-7f13-8abc-dd000000000d";
pub const AGENT_IDEM_REJ_APPLIED_1 = "0195b4ba-8d3a-7f13-8abc-dd000000000e";
pub const AGENT_IDEM_VETO_1 = "0195b4ba-8d3a-7f13-8abc-dd000000000f";

pub const AGENT_PROP_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000010";
pub const AGENT_PROP_2 = "0195b4ba-8d3a-7f13-8abc-dd0000000011";
pub const AGENT_PROP_3 = "0195b4ba-8d3a-7f13-8abc-dd0000000012";
pub const AGENT_PROP_4 = "0195b4ba-8d3a-7f13-8abc-dd0000000013";
pub const AGENT_PROP_5 = "0195b4ba-8d3a-7f13-8abc-dd0000000014";

pub const AGENT_PROP_ARCH_IDEM_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000015";
pub const AGENT_PROP_ARCH_MANUAL_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000016";
pub const AGENT_PROP_ARCH_VETO_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000017";

pub const AGENT_PROP_AUTO_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000018";
pub const AGENT_PROP_AUTO_2 = "0195b4ba-8d3a-7f13-8abc-dd0000000019";
pub const AGENT_PROP_AUTO_EQ_1 = "0195b4ba-8d3a-7f13-8abc-dd000000001a";

pub const AGENT_PROP_EXPIRY_EXACT_1 = "0195b4ba-8d3a-7f13-8abc-dd000000001b";
pub const AGENT_PROP_GUARD_1 = "0195b4ba-8d3a-7f13-8abc-dd000000001c";

pub const AGENT_PROP_MANUAL_1 = "0195b4ba-8d3a-7f13-8abc-dd000000001d";
pub const AGENT_PROP_MANUAL_2 = "0195b4ba-8d3a-7f13-8abc-dd000000001e";
pub const AGENT_PROP_MANUAL_3 = "0195b4ba-8d3a-7f13-8abc-dd000000001f";
pub const AGENT_PROP_MANUAL_4 = "0195b4ba-8d3a-7f13-8abc-dd0000000020";

pub const AGENT_PROP_MANUAL_ACTIVATE_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000021";
pub const AGENT_PROP_MANUAL_COMPILE_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000022";
pub const AGENT_PROP_MANUAL_DUP_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000023";

pub const AGENT_PROP_TEAM_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000024";
pub const AGENT_PROP_THRESHOLD_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000025";
pub const AGENT_PROP_TRUSTED_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000026";

pub const AGENT_PROP_VETO_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000027";
pub const AGENT_PROP_VETO_MANUAL_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000028";

pub const AGENT_REPORT_1 = "0195b4ba-8d3a-7f13-8abc-dd0000000029";

// e2e agents (used in proposals_e2e_test.zig)
pub const AGENT_E2E_1 = "0195b4ba-8d3a-7f13-8abc-dd000000002a";
pub const AGENT_E2E_2 = "0195b4ba-8d3a-7f13-8abc-dd000000002b";
pub const AGENT_E2E_3 = "0195b4ba-8d3a-7f13-8abc-dd000000002c";

// revert agents (used in proposals_revert_validation_test.zig)
pub const AGENT_PROP_REVERT_INSERT = "0195b4ba-8d3a-7f13-8abc-dd000000002d";
pub const AGENT_PROP_REVERT_BINDING = "0195b4ba-8d3a-7f13-8abc-dd000000002e";

// ── Workspace constants ──────────────────────────────────────────────────────
// guard test 1 needs its own workspace (was "ws_guard_1" — not a valid UUID)
pub const WS_GUARD_1 = "0195b4ba-8d3a-7f13-8abc-cc0000000401";

// ── Seed / teardown ─────────────────────────────────────────────────────────

/// Pre-clean + seed tenant + workspace. Idempotent.
pub fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    teardownWorkspace(conn, workspace_id);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
}

/// Teardown workspace (CASCADE removes child rows) then tenant.
pub fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}

/// Teardown workspace only (no tenant removal).
/// Use when multiple workspaces share one tenant in the same test.
pub fn teardownWorkspace(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
}
