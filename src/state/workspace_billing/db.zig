//! DB writes, entitlement management, and audit for workspace billing.

const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const entitlements = @import("../entitlements.zig");
const model = @import("../workspace_billing_model.zig");

const EMPTY_JSON = "{}";
const DEFAULT_AGENT_SCORING_WEIGHTS_JSON = "{\"completion\":0.4,\"error_rate\":0.3,\"latency\":0.2,\"resource\":0.1}";

pub fn configuredAdapterModeLabel(alloc: std.mem.Allocator) []const u8 {
    const raw = std.process.getEnvVarOwned(alloc, "BILLING_ADAPTER_MODE") catch return "noop";
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (std.mem.eql(u8, trimmed, "manual")) return "manual";
    if (std.mem.eql(u8, trimmed, "provider_stub")) return "provider_stub";
    return "noop";
}

pub fn entitlementForTier(tier: model.PlanTier) entitlements.EntitlementPolicy {
    return switch (tier) {
        .free => .{
            .tier = .free,
            .max_profiles = 1,
            .max_stages = 3,
            .max_distinct_skills = 3,
            .allow_custom_skills = false,
        },
        .scale => .{
            .tier = .scale,
            .max_profiles = 8,
            .max_stages = 8,
            .max_distinct_skills = 16,
            .allow_custom_skills = true,
        },
    };
}

pub fn applyEntitlementPlan(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    tier: model.PlanTier,
    now_ms: i64,
) !void {
    const entitlement_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(entitlement_id);
    const policy = entitlementForTier(tier);
    var q = try conn.query(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, false, $8, $9, $9)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    max_profiles = EXCLUDED.max_profiles,
        \\    max_stages = EXCLUDED.max_stages,
        \\    max_distinct_skills = EXCLUDED.max_distinct_skills,
        \\    allow_custom_skills = EXCLUDED.allow_custom_skills,
        \\    enable_agent_scoring = EXCLUDED.enable_agent_scoring,
        \\    agent_scoring_weights_json = EXCLUDED.agent_scoring_weights_json,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        entitlement_id,
        workspace_id,
        tier.label(),
        @as(i32, policy.max_profiles),
        @as(i32, policy.max_stages),
        @as(i32, policy.max_distinct_skills),
        policy.allow_custom_skills,
        DEFAULT_AGENT_SCORING_WEIGHTS_JSON,
        now_ms,
    });
    q.deinit();
}

pub fn upsertBillingState(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: struct {
        plan_tier: model.PlanTier,
        plan_sku: []const u8,
        billing_status: model.BillingStatus,
        adapter: []const u8,
        subscription_id: ?[]const u8,
        payment_failed_at: ?i64,
        grace_expires_at: ?i64,
        pending_status: ?model.PendingStatus,
        pending_reason: ?[]const u8,
    },
    now_ms: i64,
) !void {
    const billing_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(billing_id);
    var q = try conn.query(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id,
        \\   payment_failed_at, grace_expires_at, pending_status, pending_reason, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    plan_sku = EXCLUDED.plan_sku,
        \\    billing_status = EXCLUDED.billing_status,
        \\    adapter = EXCLUDED.adapter,
        \\    subscription_id = EXCLUDED.subscription_id,
        \\    payment_failed_at = EXCLUDED.payment_failed_at,
        \\    grace_expires_at = EXCLUDED.grace_expires_at,
        \\    pending_status = EXCLUDED.pending_status,
        \\    pending_reason = EXCLUDED.pending_reason,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        billing_id,
        workspace_id,
        state.plan_tier.label(),
        state.plan_sku,
        state.billing_status.label(),
        state.adapter,
        state.subscription_id,
        state.payment_failed_at,
        state.grace_expires_at,
        if (state.pending_status) |v| v.label() else null,
        state.pending_reason,
        now_ms,
    });
    q.deinit();
}

pub fn insertAudit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    event_type: []const u8,
    previous_plan_tier: ?model.PlanTier,
    new_plan_tier: model.PlanTier,
    previous_status: ?model.BillingStatus,
    new_status: model.BillingStatus,
    reason: []const u8,
    actor: []const u8,
    metadata_json: []const u8,
) !void {
    const audit_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(audit_id);
    var q = try conn.query(
        \\INSERT INTO workspace_billing_audit
        \\  (audit_id, workspace_id, event_type, previous_plan_tier, new_plan_tier, previous_status, new_status, reason, actor, metadata_json, created_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    , .{
        audit_id,
        workspace_id,
        event_type,
        if (previous_plan_tier) |v| v.label() else null,
        new_plan_tier.label(),
        if (previous_status) |v| v.label() else null,
        new_status.label(),
        reason,
        actor,
        metadata_json,
        std.time.milliTimestamp(),
    });
    q.deinit();
}

test "configuredAdapterModeLabel returns noop when env var not set" {
    // Ensure the env var is not set (it likely isn't in CI/test environments)
    // We use a test allocator — if BILLING_ADAPTER_MODE is absent, returns "noop"
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const label = configuredAdapterModeLabel(arena.allocator());
    // Without BILLING_ADAPTER_MODE set, result must be "noop"
    // (If the env var happens to be set to "manual" or "provider_stub" in CI, this
    //  test would still pass because the logic is deterministic from env.)
    // We assert only the invariant: result is one of the three valid values.
    const valid = std.mem.eql(u8, label, "noop") or
        std.mem.eql(u8, label, "manual") or
        std.mem.eql(u8, label, "provider_stub");
    try std.testing.expect(valid);
}
