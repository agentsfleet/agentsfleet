const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const workspace_billing = @import("./workspace_billing.zig");

const BillingStatus = workspace_billing.BillingStatus;
const PlanTier = workspace_billing.PlanTier;

const base = @import("../db/test_fixtures.zig");
const uc3 = @import("../db/test_fixtures_uc3.zig");

test "upgrade applies scale entitlement deterministically" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_UPGRADE);
    defer uc3.teardown(db_ctx.conn, uc3.WS_UPGRADE);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_UPGRADE, "test");

    const upgraded = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_UPGRADE, .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });
    defer std.testing.allocator.free(upgraded.plan_sku);
    defer if (upgraded.subscription_id) |v| std.testing.allocator.free(v);

    try std.testing.expectEqual(PlanTier.scale, upgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, upgraded.billing_status);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills FROM workspace_entitlements WHERE workspace_id = $1",
        .{uc3.WS_UPGRADE},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("SCALE", try row.get([]const u8, 0));
    try std.testing.expectEqual(@as(i32, 8), try row.get(i32, 1));
    try std.testing.expectEqual(@as(i32, 8), try row.get(i32, 2));
    try std.testing.expectEqual(@as(i32, 16), try row.get(i32, 3));
    try std.testing.expect(try row.get(bool, 4));
}

test "payment failure transitions to grace then downgrade policy" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_GRACE);
    defer uc3.teardown(db_ctx.conn, uc3.WS_GRACE);

    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_GRACE, .{
            .subscription_id = "sub_scale_123",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const grace = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, uc3.WS_GRACE, .{
        .event = .payment_failed,
        .reason = "invoice_failed",
        .actor = "billing_adapter",
    });
    defer std.testing.allocator.free(grace.plan_sku);
    defer if (grace.subscription_id) |v| std.testing.allocator.free(v);
    try std.testing.expectEqual(PlanTier.scale, grace.plan_tier);
    try std.testing.expectEqual(BillingStatus.grace, grace.billing_status);
    try std.testing.expect(grace.grace_expires_at != null);

    const downgraded = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, uc3.WS_GRACE, grace.grace_expires_at.? + 1, "sync");
    defer std.testing.allocator.free(downgraded.plan_sku);
    try std.testing.expectEqual(PlanTier.free, downgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.downgraded, downgraded.billing_status);
    try std.testing.expect(downgraded.subscription_id == null);
}

test "billing sync remains stable across repeated sync cycles" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_SYNC);
    defer uc3.teardown(db_ctx.conn, uc3.WS_SYNC);

    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_SYNC, .{
            .subscription_id = "sub_scale_123",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const first = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, uc3.WS_SYNC, 200, "sync");
    defer std.testing.allocator.free(first.plan_sku);
    defer if (first.subscription_id) |v| std.testing.allocator.free(v);

    const second = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, uc3.WS_SYNC, 201, "sync");
    defer std.testing.allocator.free(second.plan_sku);
    defer if (second.subscription_id) |v| std.testing.allocator.free(v);

    try std.testing.expectEqual(PlanTier.scale, first.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, first.billing_status);
    try std.testing.expectEqual(PlanTier.scale, second.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, second.billing_status);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM workspace_billing_audit WHERE event_type = 'SCALE_ACTIVATED' AND workspace_id = $1",
        .{uc3.WS_SYNC},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

test "missing billing state provisions free deterministically on reconcile" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_MISSING);
    defer uc3.teardown(db_ctx.conn, uc3.WS_MISSING);

    const state = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, uc3.WS_MISSING, 50, "sync");
    defer std.testing.allocator.free(state.plan_sku);
    try std.testing.expectEqual(PlanTier.free, state.plan_tier);
    try std.testing.expectEqual(BillingStatus.active, state.billing_status);
    try std.testing.expect(state.subscription_id == null);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT plan_tier, billing_status FROM workspace_billing_state WHERE workspace_id = $1",
        .{uc3.WS_MISSING},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("FREE", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings("ACTIVE", try row.get([]const u8, 1));
}

test "manual scale to free downgrade is deterministic" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_MANUAL_DOWNGRADE);
    defer uc3.teardown(db_ctx.conn, uc3.WS_MANUAL_DOWNGRADE);

    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_MANUAL_DOWNGRADE, .{
            .subscription_id = "sub_scale_456",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const state = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, uc3.WS_MANUAL_DOWNGRADE, .{
        .event = .downgrade_to_free,
        .reason = "operator_request",
        .actor = "billing_adapter",
    });
    defer std.testing.allocator.free(state.plan_sku);
    try std.testing.expectEqual(PlanTier.free, state.plan_tier);
    try std.testing.expectEqual(BillingStatus.downgraded, state.billing_status);
    try std.testing.expect(state.subscription_id == null);
}

test "downgraded workspace can become a paying customer again" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_RESUBSCRIBE);
    defer uc3.teardown(db_ctx.conn, uc3.WS_RESUBSCRIBE);

    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_RESUBSCRIBE, .{
            .subscription_id = "sub_scale_789",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const downgraded = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, uc3.WS_RESUBSCRIBE, .{
        .event = .downgrade_to_free,
        .reason = "operator_request",
        .actor = "billing_adapter",
    });
    defer std.testing.allocator.free(downgraded.plan_sku);

    const upgraded = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_RESUBSCRIBE, .{
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
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_CASCADE);
    defer base.teardownTenant(db_ctx.conn);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_CASCADE, "test");

    _ = try db_ctx.conn.exec(
        "DELETE FROM workspaces WHERE workspace_id = $1",
        .{uc3.WS_CASCADE},
    );

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_state WHERE workspace_id = $1",
            .{uc3.WS_CASCADE},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_audit WHERE workspace_id = $1",
            .{uc3.WS_CASCADE},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
}

test "free workspace creation limit blocks second non-scale workspace for same tenant" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seedWithTenant(db_ctx.conn, uc3.WS_LIMIT_BLOCK, uc3.TENANT_LIMIT_BLOCK, "limit-block-tenant");
    defer uc3.teardownWithTenant(db_ctx.conn, uc3.WS_LIMIT_BLOCK, uc3.TENANT_LIMIT_BLOCK);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_LIMIT_BLOCK, "test");

    try std.testing.expectError(
        error.FreeWorkspaceLimitExceeded,
        workspace_billing.enforceFreeWorkspaceCreationAllowed(
            db_ctx.conn,
            uc3.TENANT_LIMIT_BLOCK,
            null,
        ),
    );
}

test "free workspace creation limit ignores existing scale workspace and same callback workspace" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seedWithTenant(db_ctx.conn, uc3.WS_LIMIT_IGNORE, uc3.TENANT_LIMIT_IGNORE, "limit-ignore-tenant");
    defer uc3.teardownWithTenant(db_ctx.conn, uc3.WS_LIMIT_IGNORE, uc3.TENANT_LIMIT_IGNORE);

    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_LIMIT_IGNORE, .{
            .subscription_id = "sub_scale_999",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        uc3.TENANT_LIMIT_IGNORE,
        null,
    );
    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        uc3.TENANT_LIMIT_IGNORE,
        uc3.WS_LIMIT_IGNORE,
    );
}

test "upgrade with empty subscription_id returns InvalidSubscriptionId" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_EMPTY_SUB);
    defer uc3.teardown(db_ctx.conn, uc3.WS_EMPTY_SUB);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_EMPTY_SUB, "test");

    try std.testing.expectError(
        error.InvalidSubscriptionId,
        workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_EMPTY_SUB, .{
            .subscription_id = "",
            .actor = "test",
        }),
    );
}

test "upgrade with whitespace-only subscription_id returns InvalidSubscriptionId" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seed(db_ctx.conn, uc3.WS_WHITESPACE_SUB);
    defer uc3.teardown(db_ctx.conn, uc3.WS_WHITESPACE_SUB);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_WHITESPACE_SUB, "test");

    try std.testing.expectError(
        error.InvalidSubscriptionId,
        workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, uc3.WS_WHITESPACE_SUB, .{
            .subscription_id = "  \t\r\n  ",
            .actor = "test",
        }),
    );
}

test "enforceFreeWorkspaceCreationAllowed excludes the workspace itself" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc3.seedWithTenant(db_ctx.conn, uc3.WS_EXCLUDE_SELF, uc3.TENANT_EXCLUDE_SELF, "exclude-self-tenant");
    defer uc3.teardownWithTenant(db_ctx.conn, uc3.WS_EXCLUDE_SELF, uc3.TENANT_EXCLUDE_SELF);

    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, uc3.WS_EXCLUDE_SELF, "test");

    // Without exclusion: same-tenant free workspace → limit exceeded
    try std.testing.expectError(
        error.FreeWorkspaceLimitExceeded,
        workspace_billing.enforceFreeWorkspaceCreationAllowed(
            db_ctx.conn,
            uc3.TENANT_EXCLUDE_SELF,
            null,
        ),
    );

    // With exclusion of that workspace itself → allowed (T7 regression parity for update path)
    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        uc3.TENANT_EXCLUDE_SELF,
        uc3.WS_EXCLUDE_SELF,
    );
}
