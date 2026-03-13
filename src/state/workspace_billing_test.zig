const std = @import("std");
const pg = @import("pg");
const workspace_billing = @import("./workspace_billing.zig");

const BillingStatus = workspace_billing.BillingStatus;
const PlanTier = workspace_billing.PlanTier;

test "upgrade applies scale entitlement deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    _ = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    _ = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .subscription_id = "sub_scale_123",
        .actor = "test",
    });

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13");
    _ = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13", .{
        .subscription_id = "sub_scale_456",
        .actor = "test",
    });

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14");
    _ = try workspace_billing.upgradeWorkspaceToScale(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f14", .{
        .subscription_id = "sub_scale_789",
        .actor = "test",
    });

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempWorkspaceBillingTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15");
    try workspace_billing.provisionFreeWorkspace(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f15", "test");

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
