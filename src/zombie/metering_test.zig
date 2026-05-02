// Tests for src/zombie/metering.zig — tenant-scoped billing path.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

// Segment 5 (aa05*) identifies M11_005 metering workspaces.
const WS_SCALE_EXEMPT = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_DEDUCTED = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_EXHAUSTED = "0195b4ba-8d3a-7f13-8abc-aa0500000003";

test "DeductionResult variants compile and pattern-match" {
    const r1: metering.DeductionResult = .{ .deducted = 7 };
    const r2: metering.DeductionResult = .{ .exempt = {} };
    const r3: metering.DeductionResult = .{ .exhausted = {} };
    const r4: metering.DeductionResult = .{ .db_error = {} };

    try std.testing.expectEqual(@as(i64, 7), switch (r1) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });
    switch (r2) {
        .exempt => {},
        else => return error.TestExpectedEqual,
    }
    switch (r3) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }
    switch (r4) {
        .db_error => {},
        else => return error.TestExpectedEqual,
    }
}

test "scale-tier deduction is exempt (no DB writes)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_SCALE_EXEMPT);
    defer uc1.teardown(db_ctx.conn, WS_SCALE_EXEMPT);

    // Provision the tenant billing row at scale tier. Metering must short-circuit
    // without touching the balance.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, "scale", "scale_default", 0, "test_scale");

    const result = metering.deductZombieUsage(db_ctx.conn, uc1.TENANT_ID, .{
        .zombie_id = "z",
        .workspace_id = WS_SCALE_EXEMPT,
        .event_id = "e",
        .agent_seconds = 30,
        .token_count = 0,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 0,
    }, .scale);

    switch (result) {
        .exempt => {},
        else => return error.TestExpectedEqual,
    }
}

test "free-tier deduction decrements tenant balance" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_DEDUCTED);
    defer uc1.teardown(db_ctx.conn, WS_DEDUCTED);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const result = metering.deductZombieUsage(db_ctx.conn, uc1.TENANT_ID, .{
        .zombie_id = "z",
        .workspace_id = WS_DEDUCTED,
        .event_id = "e",
        .agent_seconds = 5, // 5¢ under FREE_PLAN_CENTS_PER_AGENT_SECOND=1
        .token_count = 0,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 0,
    }, .free);

    switch (result) {
        .deducted => |c| try std.testing.expectEqual(@as(i64, 5), c),
        else => return error.TestExpectedEqual,
    }

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 995), row.balance_cents);
}

test "free-tier deduction on exhausted balance returns .exhausted" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_EXHAUSTED);
    defer uc1.teardown(db_ctx.conn, WS_EXHAUSTED);

    // Provision at 3¢, then try to debit 5¢.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, "free", "free_default", 3, "test_exhausted");

    const result = metering.deductZombieUsage(db_ctx.conn, uc1.TENANT_ID, .{
        .zombie_id = "z",
        .workspace_id = WS_EXHAUSTED,
        .event_id = "e",
        .agent_seconds = 5,
        .token_count = 0,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 0,
    }, .free);

    switch (result) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }

    // Balance must stay at 3 — atomic UPDATE ... WHERE guard prevents partial debit.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 3), row.balance_cents);
}
