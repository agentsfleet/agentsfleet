/// test_fixtures_uc3.zig — UC3: Billing integration fixtures.
///
/// Covers:
///   src/state/billing_test.zig          — usage metering + credit deduction
///   src/state/workspace_billing_test.zig — plan lifecycle (provision/upgrade/grace/downgrade)
///
/// Tables seeded: tenants, workspaces (and optionally specs, runs for billing_test)
/// Tables cleaned via CASCADE on workspace delete:
///   workspace_billing_state, workspace_billing_audit, workspace_credit_state,
///   workspace_credit_audit, usage_ledger, billing_delivery_outbox, workspace_entitlements
///
/// Usage per test:
///
///   try uc3.seed(conn, uc3.WS_UPGRADE);
///   defer uc3.teardown(conn, uc3.WS_UPGRADE);
const std = @import("std");
const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TEST_TENANT_ID = base.TEST_TENANT_ID;

// ── Workspace IDs for workspace_billing_test.zig ────────────────────────
// Segment 5 prefix cc01–cc0c identifies UC3 billing-lifecycle workspaces.

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

// ── Tenant IDs for multi-tenant free-workspace-limit tests ──────────────

pub const TENANT_LIMIT_BLOCK = "0195b4ba-8d3a-7f13-8abc-cc0000000051";
pub const TENANT_LIMIT_IGNORE = "0195b4ba-8d3a-7f13-8abc-cc0000000052";
pub const TENANT_EXCLUDE_SELF = "0195b4ba-8d3a-7f13-8abc-cc0000000053";

// ── Workspace IDs for billing_test.zig ──────────────────────────────────
// Prefix cc11–cc12 identifies UC3 metering workspaces.

pub const WS_BT_FREE = "0195b4ba-8d3a-7f13-8abc-cc0000000011";
pub const WS_BT_SCALE = "0195b4ba-8d3a-7f13-8abc-cc0000000012";

// Spec + run IDs for billing_test (usage_ledger + billing_delivery_outbox have FK to runs).
pub const SPEC_BT_FREE = "0195b4ba-8d3a-7f13-8abc-cc0000000021";
pub const SPEC_BT_SCALE = "0195b4ba-8d3a-7f13-8abc-cc0000000022";
pub const RUN_BT_COMPLETED = "0195b4ba-8d3a-7f13-8abc-cc0000000031";
pub const RUN_BT_NON_BILLABLE = "0195b4ba-8d3a-7f13-8abc-cc0000000032";
pub const RUN_BT_SCALE = "0195b4ba-8d3a-7f13-8abc-cc0000000033";

// IDs for billing_reconciler.zig test (prefix cc13/cc23/cc34/cc44).
pub const WS_RECONCILER = "0195b4ba-8d3a-7f13-8abc-cc0000000013";
pub const SPEC_RECONCILER = "0195b4ba-8d3a-7f13-8abc-cc0000000023";
pub const RUN_RECONCILER = "0195b4ba-8d3a-7f13-8abc-cc0000000034";
pub const OUTBOX_RECONCILER = "0195b4ba-8d3a-7f13-8abc-cc0000000044";

// ── Seed / teardown ─────────────────────────────────────────────────────

/// Seed canonical tenant + one workspace. For workspace_billing_test.zig tests.
/// Pre-cleans the workspace to handle stale state from interrupted test runs.
pub fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
}

/// Seed with a custom tenant (for free-workspace-limit tests).
/// Pre-cleans the workspace to handle stale state from interrupted test runs.
pub fn seedWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8, tenant_name: []const u8) !void {
    base.teardownWorkspace(conn, workspace_id);
    try base.seedTenantById(conn, tenant_id, tenant_name);
    try base.seedWorkspaceWithTenant(conn, workspace_id, tenant_id);
}

/// Seed workspace + spec + runs for billing_test.zig tests that insert into usage_ledger.
pub fn seedWithRuns(conn: *pg.Conn, workspace_id: []const u8, spec_id: []const u8, run_ids: []const []const u8) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, workspace_id);
    try base.seedSpec(conn, spec_id, workspace_id);
    for (run_ids) |run_id| {
        try base.seedRun(conn, run_id, workspace_id, spec_id);
    }
}

/// Seed workspace_billing_state for billing_test.zig (needs pre-seeded workspace).
pub fn seedBillingState(
    conn: *pg.Conn,
    billing_id: []const u8,
    workspace_id: []const u8,
    plan_tier: []const u8,
    plan_sku: []const u8,
    billing_status: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter,
        \\   subscription_id, payment_failed_at, grace_expires_at,
        \\   pending_status, pending_reason, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'noop', NULL, NULL, NULL, NULL, NULL, 1, 1)
        \\ON CONFLICT DO NOTHING
    , .{ billing_id, workspace_id, plan_tier, plan_sku, billing_status });
}

/// Seed workspace_credit_state for billing_test.zig (needs pre-seeded workspace).
pub fn seedCreditState(
    conn: *pg.Conn,
    credit_id: []const u8,
    workspace_id: []const u8,
    currency: []const u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO workspace_credit_state
        \\  (credit_id, workspace_id, currency, initial_credit_cents,
        \\   consumed_credit_cents, remaining_credit_cents, exhausted_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 1)
        \\ON CONFLICT DO NOTHING
    , .{ credit_id, workspace_id, currency, initial_credit_cents, consumed_credit_cents, remaining_credit_cents, exhausted_at });
}

/// Teardown for workspace_billing_test.zig tests (canonical tenant).
pub fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}

/// Teardown for multi-tenant tests.
pub fn teardownWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, tenant_id);
}

/// Teardown for billing_test.zig tests that seeded runs.
/// Order: billing_delivery_outbox (FK → runs) → runs → specs → workspace → tenant.
pub fn teardownWithRuns(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM billing_delivery_outbox WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch {};
    base.teardownRuns(conn, workspace_id);
    base.teardownSpecs(conn, workspace_id);
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenant(conn);
}
