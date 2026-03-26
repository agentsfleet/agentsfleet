const std = @import("std");
const pg = @import("pg");
const billing = @import("./billing_runtime.zig");
const workspace_billing = @import("./workspace_billing.zig");
const workspace_credit = @import("./workspace_credit.zig");

const base = @import("../db/test_fixtures.zig");
const uc3 = @import("../db/test_fixtures_uc3.zig");

test "ledger replay yields identical totals regardless of ordering" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const a = [_]billing.UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
    };
    const b = [_]billing.UsageSnapshot{
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
    };

    const t1 = try billing.aggregateBillableQuantityFromSnapshots(alloc, &a);
    const t2 = try billing.aggregateBillableQuantityFromSnapshots(alloc, &b);
    try std.testing.expectEqual(t1, t2);
    try std.testing.expectEqual(@as(u64, 60), t1);
}

test "duplicate events do not double-charge" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const events = [_]billing.UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k2", .is_billable = true, .billable_quantity = 5 },
    };
    const total = try billing.aggregateBillableQuantityFromSnapshots(alloc, &events);
    try std.testing.expectEqual(@as(u64, 15), total);
}

test "completed runs are metered and non-billable runs are not charged" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const run_ids = [_][]const u8{ uc3.RUN_BT_COMPLETED, uc3.RUN_BT_NON_BILLABLE };
    try uc3.seedWithRuns(db_ctx.conn, uc3.WS_BT_FREE, uc3.SPEC_BT_FREE, &run_ids);
    defer uc3.teardownWithRuns(db_ctx.conn, uc3.WS_BT_FREE);

    try uc3.seedBillingState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-cc0000000041",
        uc3.WS_BT_FREE,
        "FREE",
        workspace_billing.FREE_PLAN_SKU,
        "ACTIVE",
    );
    try uc3.seedCreditState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-cc0000000042",
        uc3.WS_BT_FREE,
        workspace_credit.CREDIT_CURRENCY,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        0,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        null,
    );

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        uc3.WS_BT_FREE,
        uc3.RUN_BT_COMPLETED,
        1,
        "plan",
        .echo,
        10,
        12,
    );
    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        uc3.WS_BT_FREE,
        uc3.RUN_BT_COMPLETED,
        1,
        "implement",
        .scout,
        20,
        30,
    );

    try billing.finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        uc3.WS_BT_FREE,
        uc3.RUN_BT_COMPLETED,
        1,
        .completed,
    );

    {
        var q = try db_ctx.conn.query(
            \\SELECT billable_quantity, is_billable
            \\FROM usage_ledger
            \\WHERE run_id = $1 AND lifecycle_event = 'run_completed'
            \\LIMIT 1
        , .{uc3.RUN_BT_COMPLETED});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 42), try row.get(i64, 0));
        try std.testing.expect(try row.get(bool, 1));
    }

    {
        var q = try db_ctx.conn.query(
            \\SELECT consumed_credit_cents, remaining_credit_cents
            \\FROM workspace_credit_state
            \\WHERE workspace_id = $1
            \\LIMIT 1
        , .{uc3.WS_BT_FREE});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 42), try row.get(i64, 0));
        try std.testing.expectEqual(workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS - 42, try row.get(i64, 1));
    }

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{uc3.RUN_BT_COMPLETED},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        uc3.WS_BT_FREE,
        uc3.RUN_BT_NON_BILLABLE,
        1,
        "plan",
        .echo,
        8,
        9,
    );
    try billing.finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        uc3.WS_BT_FREE,
        uc3.RUN_BT_NON_BILLABLE,
        1,
        .non_billable,
    );

    {
        var q = try db_ctx.conn.query(
            \\SELECT billable_quantity, is_billable
            \\FROM usage_ledger
            \\WHERE run_id = $1 AND lifecycle_event = 'run_not_billable'
            \\LIMIT 1
        , .{uc3.RUN_BT_NON_BILLABLE});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        try std.testing.expect(!try row.get(bool, 1));
    }

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{uc3.RUN_BT_NON_BILLABLE},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
}

test "scale runs queue billing delivery without touching free-credit ledger" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const run_ids = [_][]const u8{uc3.RUN_BT_SCALE};
    try uc3.seedWithRuns(db_ctx.conn, uc3.WS_BT_SCALE, uc3.SPEC_BT_SCALE, &run_ids);
    defer uc3.teardownWithRuns(db_ctx.conn, uc3.WS_BT_SCALE);

    try uc3.seedBillingState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-cc0000000043",
        uc3.WS_BT_SCALE,
        "SCALE",
        workspace_billing.SCALE_PLAN_SKU,
        "ACTIVE",
    );
    try uc3.seedCreditState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-cc0000000044",
        uc3.WS_BT_SCALE,
        workspace_credit.CREDIT_CURRENCY,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        0,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        null,
    );

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        uc3.WS_BT_SCALE,
        uc3.RUN_BT_SCALE,
        1,
        "plan",
        .echo,
        7,
        15,
    );

    try billing.finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        uc3.WS_BT_SCALE,
        uc3.RUN_BT_SCALE,
        1,
        .completed,
    );

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{uc3.RUN_BT_SCALE},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
    }

    {
        var q = try db_ctx.conn.query(
            \\SELECT consumed_credit_cents, remaining_credit_cents
            \\FROM workspace_credit_state
            \\WHERE workspace_id = $1
            \\LIMIT 1
        , .{uc3.WS_BT_SCALE});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        try std.testing.expectEqual(workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS, try row.get(i64, 1));
    }
}
