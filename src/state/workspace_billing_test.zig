const std = @import("std");
const pg = @import("pg");
const workspace_billing = @import("./workspace_billing.zig");

const BillingStatus = workspace_billing.BillingStatus;
const PlanTier = workspace_billing.PlanTier;

test "upgrade applies scale entitlement deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "test");

    const upgraded = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
            .subscription_id = "sub_scale_123",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const grace = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .event = .payment_failed,
        .reason = "invoice_failed",
        .actor = "billing_adapter",
    });
    defer std.testing.allocator.free(grace.plan_sku);
    defer if (grace.subscription_id) |v| std.testing.allocator.free(v);
    try std.testing.expectEqual(PlanTier.scale, grace.plan_tier);
    try std.testing.expectEqual(BillingStatus.grace, grace.billing_status);
    try std.testing.expect(grace.grace_expires_at != null);

    const downgraded = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", grace.grace_expires_at.? + 1, "sync");
    defer std.testing.allocator.free(downgraded.plan_sku);
    try std.testing.expectEqual(PlanTier.free, downgraded.plan_tier);
    try std.testing.expectEqual(BillingStatus.downgraded, downgraded.billing_status);
    try std.testing.expect(downgraded.subscription_id == null);
}

test "billing sync remains stable across repeated sync cycles" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
            .subscription_id = "sub_scale_123",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const first = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", 200, "sync");
    defer std.testing.allocator.free(first.plan_sku);
    defer if (first.subscription_id) |v| std.testing.allocator.free(v);

    const second = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", 201, "sync");
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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12");

    const state = try workspace_billing.reconcileWorkspaceBilling(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12", 50, "sync");
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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13");
    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13", .{
            .subscription_id = "sub_scale_456",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const state = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13", .{
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
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14");
    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
            .subscription_id = "sub_scale_789",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    const downgraded = try workspace_billing.applyBillingLifecycleEvent(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
        .event = .downgrade_to_free,
        .reason = "operator_request",
        .actor = "billing_adapter",
    });
    defer std.testing.allocator.free(downgraded.plan_sku);

    const upgraded = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15", "test");

    _ = try db_ctx.conn.exec(
        "DELETE FROM workspaces WHERE workspace_id = $1",
        .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
    );

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_state WHERE workspace_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        q.drain() catch {};
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM workspace_billing_audit WHERE workspace_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        q.drain() catch {};
    }
}

test "free workspace creation limit blocks second non-scale workspace for same tenant" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspaceWithTenant(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f16", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa1");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f16", "test");

    try std.testing.expectError(
        error.FreeWorkspaceLimitExceeded,
        workspace_billing.enforceFreeWorkspaceCreationAllowed(
            db_ctx.conn,
            "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa1",
            null,
        ),
    );
}

test "free workspace creation limit ignores existing scale workspace and same callback workspace" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspaceWithTenant(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f17", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2");
    {
        const sv = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f17", .{
            .subscription_id = "sub_scale_999",
            .actor = "test",
        });
        std.testing.allocator.free(sv.plan_sku);
        if (sv.subscription_id) |v| std.testing.allocator.free(v);
    }

    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2",
        null,
    );
    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f17",
    );
}

fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    // Use page_allocator for opts strings so they outlive the pool. pg.Pool stores
    // shallow references to connect.host/auth strings — if the arena they come from
    // is freed first, pool.release() crashes when it calls newConnection() on a
    // non-idle conn (e.g. after an internal query failure in the test body).
    const db = @import("../db/pool.zig");
    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

fn createTempWorkspaceBillingTables(conn: *pg.Conn) !void {
    {
        _ = try conn.exec(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT
            \\)
        , .{});
    }
    {
        _ = try conn.exec(
            \\CREATE TEMP TABLE workspace_entitlements (
            \\  entitlement_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  plan_tier TEXT NOT NULL,
            \\  max_profiles INTEGER NOT NULL,
            \\  max_stages INTEGER NOT NULL,
            \\  max_distinct_skills INTEGER NOT NULL,
            \\  allow_custom_skills BOOLEAN NOT NULL,
            \\  enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE,
            \\  agent_scoring_weights_json TEXT NOT NULL DEFAULT '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\)
        , .{});
    }
    {
        _ = try conn.exec(
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
            \\)
        , .{});
    }
    {
        _ = try conn.exec(
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
            \\)
        , .{});
    }
}

test "upgrade with empty subscription_id returns InvalidSubscriptionId" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f20");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f20", "test");

    try std.testing.expectError(
        error.InvalidSubscriptionId,
        workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f20", .{
            .subscription_id = "",
            .actor = "test",
        }),
    );
}

test "upgrade with whitespace-only subscription_id returns InvalidSubscriptionId" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", "test");

    try std.testing.expectError(
        error.InvalidSubscriptionId,
        workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", .{
            .subscription_id = "  \t\r\n  ",
            .actor = "test",
        }),
    );
}

test "enforceFreeWorkspaceCreationAllowed excludes the workspace itself" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspaceWithTenant(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", "test");

    // Without exclusion: same-tenant free workspace → limit exceeded
    try std.testing.expectError(
        error.FreeWorkspaceLimitExceeded,
        workspace_billing.enforceFreeWorkspaceCreationAllowed(
            db_ctx.conn,
            "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1",
            null,
        ),
    );

    // With exclusion of that workspace itself → allowed (T7 regression parity for update path)
    try workspace_billing.enforceFreeWorkspaceCreationAllowed(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22",
    );
}

fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ($1, 'tenant-test-default')",
        .{workspace_id},
    );
}

fn seedWorkspaceWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) !void {
    _ = try conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ($1, $2)",
        .{ workspace_id, tenant_id },
    );
}
