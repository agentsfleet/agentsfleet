const std = @import("std");
const pg = @import("pg");
const entitlements = @import("./entitlements.zig");
const id_format = @import("../types/id_format.zig");

pub const FREE_PLAN_SKU = "free_v1";
pub const SCALE_PLAN_SKU = "scale_v1";
pub const DEFAULT_GRACE_PERIOD_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub const PlanTier = enum {
    free,
    scale,

    pub fn label(self: PlanTier) []const u8 {
        return switch (self) {
            .free => "FREE",
            .scale => "SCALE",
        };
    }
};

pub const BillingStatus = enum {
    active,
    grace,
    downgraded,

    pub fn label(self: BillingStatus) []const u8 {
        return switch (self) {
            .active => "ACTIVE",
            .grace => "GRACE",
            .downgraded => "DOWNGRADED",
        };
    }
};

const PendingStatus = enum {
    activate_scale,
    payment_failed,
    downgrade_to_free,

    fn label(self: PendingStatus) []const u8 {
        return switch (self) {
            .activate_scale => "ACTIVATE_SCALE",
            .payment_failed => "PAYMENT_FAILED",
            .downgrade_to_free => "DOWNGRADE_TO_FREE",
        };
    }
};

pub const StateView = struct {
    plan_tier: PlanTier,
    billing_status: BillingStatus,
    plan_sku: []const u8,
    subscription_id: ?[]const u8,
    grace_expires_at: ?i64,
};

const StateRow = struct {
    plan_tier: PlanTier,
    billing_status: BillingStatus,
    plan_sku: []u8,
    adapter: []u8,
    subscription_id: ?[]u8,
    payment_failed_at: ?i64,
    grace_expires_at: ?i64,
    pending_status: ?PendingStatus,
    pending_reason: ?[]u8,

    fn deinit(self: *StateRow, alloc: std.mem.Allocator) void {
        alloc.free(self.plan_sku);
        alloc.free(self.adapter);
        if (self.subscription_id) |v| alloc.free(v);
        if (self.pending_reason) |v| alloc.free(v);
    }
};

pub const UpgradeInput = struct {
    subscription_id: []const u8,
    actor: []const u8,
};

pub fn provisionFreeWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    try applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = FREE_PLAN_SKU,
        .billing_status = .active,
        .adapter = configuredAdapterModeLabel(alloc),
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "FREE_PROVISIONED", null, .free, null, .active, "workspace_created", actor, "{}");
}

pub fn upgradeWorkspaceToScale(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: UpgradeInput,
) !StateView {
    const trimmed_subscription = std.mem.trim(u8, input.subscription_id, " \t\r\n");
    if (trimmed_subscription.len == 0) return error.InvalidSubscriptionId;

    const previous = try ensureStateRow(conn, alloc, workspace_id);
    defer previous.deinit(alloc);

    const now_ms = std.time.milliTimestamp();
    try applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = SCALE_PLAN_SKU,
        .billing_status = .active,
        .adapter = configuredAdapterModeLabel(alloc),
        .subscription_id = trimmed_subscription,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "SCALE_ACTIVATED", previous.plan_tier, .scale, previous.billing_status, .active, "upgrade_to_scale", input.actor, "{}");
    return .{
        .plan_tier = .scale,
        .billing_status = .active,
        .plan_sku = SCALE_PLAN_SKU,
        .subscription_id = try alloc.dupe(u8, trimmed_subscription),
        .grace_expires_at = null,
    };
}

pub fn reconcileWorkspaceBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    now_ms: i64,
    actor: []const u8,
) !StateView {
    var state = try ensureStateRow(conn, alloc, workspace_id);
    defer state.deinit(alloc);

    if (state.pending_status) |pending| {
        switch (pending) {
            .activate_scale => {
                try applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
                try upsertBillingState(conn, alloc, workspace_id, .{
                    .plan_tier = .scale,
                    .plan_sku = SCALE_PLAN_SKU,
                    .billing_status = .active,
                    .adapter = state.adapter,
                    .subscription_id = state.subscription_id,
                    .payment_failed_at = null,
                    .grace_expires_at = null,
                    .pending_status = null,
                    .pending_reason = null,
                }, now_ms);
                try insertAudit(conn, alloc, workspace_id, "PENDING_SCALE_APPLIED", state.plan_tier, .scale, state.billing_status, .active, "sync_applied_scale_activation", actor, "{}");
                state.plan_tier = .scale;
                state.billing_status = .active;
                state.grace_expires_at = null;
            },
            .payment_failed => {
                if (state.plan_tier == .scale and state.billing_status != .grace) {
                    const grace_expires_at = now_ms + DEFAULT_GRACE_PERIOD_MS;
                    try upsertBillingState(conn, alloc, workspace_id, .{
                        .plan_tier = .scale,
                        .plan_sku = SCALE_PLAN_SKU,
                        .billing_status = .grace,
                        .adapter = state.adapter,
                        .subscription_id = state.subscription_id,
                        .payment_failed_at = now_ms,
                        .grace_expires_at = grace_expires_at,
                        .pending_status = null,
                        .pending_reason = null,
                    }, now_ms);
                    try insertAudit(conn, alloc, workspace_id, "PAYMENT_FAILED_GRACE_STARTED", state.plan_tier, .scale, state.billing_status, .grace, "payment_failed", actor, "{}");
                    state.plan_tier = .scale;
                    state.billing_status = .grace;
                    state.grace_expires_at = grace_expires_at;
                    state.payment_failed_at = now_ms;
                }
            },
            .downgrade_to_free => {
                try downgradeToFree(conn, alloc, workspace_id, state, now_ms, actor, "forced_downgrade");
                state.plan_tier = .free;
                state.billing_status = .downgraded;
                state.grace_expires_at = null;
                state.payment_failed_at = null;
                if (state.subscription_id) |v| {
                    alloc.free(v);
                    state.subscription_id = null;
                }
            },
        }
    }

    if (state.plan_tier == .scale and state.billing_status == .grace and state.grace_expires_at != null and now_ms >= state.grace_expires_at.?) {
        try downgradeToFree(conn, alloc, workspace_id, state, now_ms, actor, "grace_expired");
        state.plan_tier = .free;
        state.billing_status = .downgraded;
        state.grace_expires_at = null;
        state.payment_failed_at = null;
        if (state.subscription_id) |v| {
            alloc.free(v);
            state.subscription_id = null;
        }
    }

    return .{
        .plan_tier = state.plan_tier,
        .billing_status = state.billing_status,
        .plan_sku = try alloc.dupe(u8, state.plan_sku),
        .subscription_id = if (state.subscription_id) |v| try alloc.dupe(u8, v) else null,
        .grace_expires_at = state.grace_expires_at,
    };
}

fn downgradeToFree(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: StateRow,
    now_ms: i64,
    actor: []const u8,
    reason: []const u8,
) !void {
    try applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = FREE_PLAN_SKU,
        .billing_status = .downgraded,
        .adapter = state.adapter,
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "DOWNGRADED_TO_FREE", state.plan_tier, .free, state.billing_status, .downgraded, reason, actor, "{}");
}

fn configuredAdapterModeLabel(alloc: std.mem.Allocator) []const u8 {
    const raw = std.process.getEnvVarOwned(alloc, "BILLING_ADAPTER_MODE") catch return "noop";
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (std.mem.eql(u8, trimmed, "manual")) return "manual";
    if (std.mem.eql(u8, trimmed, "provider_stub")) return "provider_stub";
    return "noop";
}

fn applyEntitlementPlan(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    tier: PlanTier,
    now_ms: i64,
) !void {
    const entitlement_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(entitlement_id);
    const policy = entitlementForTier(tier);
    var q = try conn.query(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    max_profiles = EXCLUDED.max_profiles,
        \\    max_stages = EXCLUDED.max_stages,
        \\    max_distinct_skills = EXCLUDED.max_distinct_skills,
        \\    allow_custom_skills = EXCLUDED.allow_custom_skills,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        entitlement_id,
        workspace_id,
        tier.label(),
        @as(i32, policy.max_profiles),
        @as(i32, policy.max_stages),
        @as(i32, policy.max_distinct_skills),
        policy.allow_custom_skills,
        now_ms,
    });
    q.deinit();
}

fn entitlementForTier(tier: PlanTier) entitlements.EntitlementPolicy {
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

fn ensureStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !StateRow {
    const existing = loadStateRow(conn, alloc, workspace_id) catch |err| switch (err) {
        error.WorkspaceBillingStateMissing => null,
        else => return err,
    };
    if (existing) |row| return row;

    try provisionFreeWorkspace(conn, alloc, workspace_id, "system");
    return (try loadStateRow(conn, alloc, workspace_id)).?;
}

fn loadStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?StateRow {
    var q = try conn.query(
        \\SELECT plan_tier, billing_status, plan_sku, adapter, subscription_id, payment_failed_at,
        \\       grace_expires_at, pending_status, pending_reason
        \\FROM workspace_billing_state
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceBillingStateMissing;
    const plan_tier_raw = try row.get([]const u8, 0);
    const billing_status_raw = try row.get([]const u8, 1);
    const pending_status_raw = try row.get(?[]const u8, 7);
    return .{
        .plan_tier = parsePlanTier(plan_tier_raw) orelse return error.InvalidWorkspaceBillingState,
        .billing_status = parseBillingStatus(billing_status_raw) orelse return error.InvalidWorkspaceBillingState,
        .plan_sku = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .adapter = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .subscription_id = if (try row.get(?[]const u8, 4)) |v| try alloc.dupe(u8, v) else null,
        .payment_failed_at = try row.get(?i64, 5),
        .grace_expires_at = try row.get(?i64, 6),
        .pending_status = if (pending_status_raw) |v| parsePendingStatus(v) orelse return error.InvalidWorkspaceBillingState else null,
        .pending_reason = if (try row.get(?[]const u8, 8)) |v| try alloc.dupe(u8, v) else null,
    };
}

fn upsertBillingState(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: struct {
        plan_tier: PlanTier,
        plan_sku: []const u8,
        billing_status: BillingStatus,
        adapter: []const u8,
        subscription_id: ?[]const u8,
        payment_failed_at: ?i64,
        grace_expires_at: ?i64,
        pending_status: ?PendingStatus,
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

fn insertAudit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    event_type: []const u8,
    previous_plan_tier: ?PlanTier,
    new_plan_tier: PlanTier,
    previous_status: ?BillingStatus,
    new_status: BillingStatus,
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

fn parsePlanTier(raw: []const u8) ?PlanTier {
    if (std.ascii.eqlIgnoreCase(raw, "FREE")) return .free;
    if (std.ascii.eqlIgnoreCase(raw, "SCALE")) return .scale;
    return null;
}

fn parseBillingStatus(raw: []const u8) ?BillingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVE")) return .active;
    if (std.ascii.eqlIgnoreCase(raw, "GRACE")) return .grace;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADED")) return .downgraded;
    return null;
}

fn parsePendingStatus(raw: []const u8) ?PendingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVATE_SCALE")) return .activate_scale;
    if (std.ascii.eqlIgnoreCase(raw, "PAYMENT_FAILED")) return .payment_failed;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADE_TO_FREE")) return .downgrade_to_free;
    return null;
}

test "upgrade applies scale entitlement deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    try provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "test");

    const upgraded = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });
    defer std.testing.allocator.free(upgraded.plan_sku);
    defer if (upgraded.subscription_id) |v| std.testing.allocator.free(v);

    try std.testing.expectEqual(PlanTier.scale, upgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, upgraded.billing_status);

    var q = try db_ctx.conn.query(
        "SELECT plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills FROM workspace_entitlements WHERE workspace_id = $1",
        .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("SCALE", try row.get([]const u8, 0));
    try std.testing.expectEqual(@as(i32, 8), try row.get(i32, 1));
    try std.testing.expectEqual(@as(i32, 8), try row.get(i32, 2));
    try std.testing.expectEqual(@as(i32, 16), try row.get(i32, 3));
    try std.testing.expect(try row.get(bool, 4));
}

test "payment failure transitions to grace then downgrade policy" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    _ = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });

    var q = try db_ctx.conn.query(
        \\UPDATE workspace_billing_state
        \\SET pending_status = 'PAYMENT_FAILED', pending_reason = 'invoice_failed', updated_at = 10
        \\WHERE workspace_id = $1
    , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"});
    q.deinit();

    const grace = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", 100, "sync");
    defer std.testing.allocator.free(grace.plan_sku);
    defer if (grace.subscription_id) |v| std.testing.allocator.free(v);
    try std.testing.expectEqual(PlanTier.scale, grace.plan_tier);
    try std.testing.expectEqual(BillingStatus.grace, grace.billing_status);
    try std.testing.expect(grace.grace_expires_at != null);

    const downgraded = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", grace.grace_expires_at.? + 1, "sync");
    defer std.testing.allocator.free(downgraded.plan_sku);
    try std.testing.expectEqual(PlanTier.free, downgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.downgraded, downgraded.billing_status);
    try std.testing.expect(downgraded.subscription_id == null);
}

test "billing sync remains stable across repeated sync cycles" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    _ = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });

    const first = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", 200, "sync");
    defer std.testing.allocator.free(first.plan_sku);
    defer if (first.subscription_id) |v| std.testing.allocator.free(v);

    const second = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", 201, "sync");
    defer std.testing.allocator.free(second.plan_sku);
    defer if (second.subscription_id) |v| std.testing.allocator.free(v);

    try std.testing.expectEqual(PlanTier.scale, first.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, first.billing_status);
    try std.testing.expectEqual(PlanTier.scale, second.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, second.billing_status);

    var q = try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM workspace_billing_audit WHERE event_type = 'SCALE_ACTIVATED'",
        .{},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

test "missing billing state provisions free deterministically on reconcile" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12");

    const state = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12", 50, "sync");
    defer std.testing.allocator.free(state.plan_sku);
    try std.testing.expectEqual(PlanTier.free, state.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, state.billing_status);
    try std.testing.expect(state.subscription_id == null);

    var q = try db_ctx.conn.query(
        "SELECT plan_tier, billing_status FROM workspace_billing_state WHERE workspace_id = $1",
        .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12"},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("FREE", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings("ACTIVE", try row.get([]const u8, 1));
}

test "manual scale to free downgrade is deterministic" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13");
    _ = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13", .{
        .subscription_id = "sub_scale_456",
        .actor = "test",
    });

    var q = try db_ctx.conn.query(
        \\UPDATE workspace_billing_state
        \\SET pending_status = 'DOWNGRADE_TO_FREE', pending_reason = 'operator_request', updated_at = 20
        \\WHERE workspace_id = $1
    , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13"});
    q.deinit();

    const state = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13", 21, "sync");
    defer std.testing.allocator.free(state.plan_sku);
    try std.testing.expectEqual(PlanTier.free, state.plan_tier);
    try std.testing.expectEqual(BillingStatus.downgraded, state.billing_status);
    try std.testing.expect(state.subscription_id == null);
}

test "downgraded workspace can become a paying customer again" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14");
    _ = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
        .subscription_id = "sub_scale_789",
        .actor = "test",
    });

    var downgrade = try db_ctx.conn.query(
        \\UPDATE workspace_billing_state
        \\SET pending_status = 'DOWNGRADE_TO_FREE', pending_reason = 'operator_request', updated_at = 20
        \\WHERE workspace_id = $1
    , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14"});
    downgrade.deinit();
    _ = try reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", 21, "sync");

    const upgraded = try upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
        .subscription_id = "sub_scale_790",
        .actor = "test",
    });
    defer std.testing.allocator.free(upgraded.plan_sku);
    defer if (upgraded.subscription_id) |v| std.testing.allocator.free(v);

    try std.testing.expectEqual(PlanTier.scale, upgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, upgraded.billing_status);
    try std.testing.expectEqualStrings("sub_scale_790", upgraded.subscription_id.?);
}

test "workspace deletion cascades billing state cleanup" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15");
    try provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15", "test");

    var delete_q = try db_ctx.conn.query(
        "DELETE FROM workspaces WHERE workspace_id = $1",
        .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
    );
    delete_q.deinit();

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_state WHERE workspace_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_audit WHERE workspace_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
}

fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    const db = @import("../db/pool.zig");
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

fn createTempWorkspaceBillingTables(conn: *pg.Conn) !void {
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_entitlements (
            \\  entitlement_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  plan_tier TEXT NOT NULL,
            \\  max_profiles INTEGER NOT NULL,
            \\  max_stages INTEGER NOT NULL,
            \\  max_distinct_skills INTEGER NOT NULL,
            \\  allow_custom_skills BOOLEAN NOT NULL,
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_billing_state (
            \\  billing_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  plan_tier TEXT NOT NULL,
            \\  plan_sku TEXT NOT NULL,
            \\  billing_status TEXT NOT NULL,
            \\  adapter TEXT NOT NULL,
            \\  subscription_id TEXT,
            \\  payment_failed_at BIGINT,
            \\  grace_expires_at BIGINT,
            \\  pending_status TEXT,
            \\  pending_reason TEXT,
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_billing_audit (
            \\  audit_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  event_type TEXT NOT NULL,
            \\  previous_plan_tier TEXT,
            \\  new_plan_tier TEXT NOT NULL,
            \\  previous_status TEXT,
            \\  new_status TEXT NOT NULL,
            \\  reason TEXT NOT NULL,
            \\  actor TEXT NOT NULL,
            \\  metadata_json TEXT NOT NULL,
            \\  created_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
}

fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    var q = try conn.query(
        "INSERT INTO workspaces (workspace_id) VALUES ($1)",
        .{workspace_id},
    );
    q.deinit();
}
