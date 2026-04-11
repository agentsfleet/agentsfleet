// Tests for workspace_credit.zig — extracted per RULE FLL (350-line gate).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const credit = @import("workspace_credit.zig");
const store = @import("workspace_credit_store.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const openTestConn = base.openTestConn;

const errorCode = credit.errorCode;
const errorMessage = credit.errorMessage;
const runtimeUsageCostCents = credit.runtimeUsageCostCents;
const provisionWorkspaceCredit = credit.provisionWorkspaceCredit;
const getOrProvisionWorkspaceCredit = credit.getOrProvisionWorkspaceCredit;
const enforceExecutionAllowed = credit.enforceExecutionAllowed;
const deductCompletedRuntimeUsage = credit.deductCompletedRuntimeUsage;
const FREE_PLAN_INITIAL_CREDIT_CENTS = credit.FREE_PLAN_INITIAL_CREDIT_CENTS;
const CREDIT_CURRENCY = credit.CREDIT_CURRENCY;

// T1 + T2 — runtimeUsageCostCents: zero, normal, and u64 overflow all clamp safely
test "runtimeUsageCostCents handles zero, normal, and overflow inputs" {
    try std.testing.expectEqual(@as(i64, 0), runtimeUsageCostCents(0));
    try std.testing.expectEqual(@as(i64, 42), runtimeUsageCostCents(42));
    try std.testing.expectEqual(std.math.maxInt(i64), runtimeUsageCostCents(std.math.maxInt(u64)));
}

// T10 — errorCode and errorMessage cover the CreditExhausted branch and unknown passthrough
test "errorCode and errorMessage map CreditExhausted; return null for unknown errors" {
    try std.testing.expect(errorCode(error.CreditExhausted) != null);
    try std.testing.expectEqual(@as(?[]const u8, null), errorCode(error.OutOfMemory));
    const msg = errorMessage(error.CreditExhausted);
    try std.testing.expect(msg != null);
    try std.testing.expect(std.mem.indexOf(u8, msg.?, "Free plan") != null);
    try std.testing.expectEqual(@as(?[]const u8, null), errorMessage(error.OutOfMemory));
}

test "provisionWorkspaceCredit grants initial free credit deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_PROVISION);
    defer uc1.teardown(db_ctx.conn, uc1.WS_PROVISION);

    try provisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, uc1.WS_PROVISION, "test");

    const credit_view = try getOrProvisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, uc1.WS_PROVISION);
    defer std.testing.allocator.free(credit_view.currency);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit_view.initial_credit_cents);
    try std.testing.expectEqual(@as(i64, 0), credit_view.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit_view.remaining_credit_cents);
}

test "enforceExecutionAllowed blocks exhausted free plan and allows scale" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_ENFORCE);
    defer uc1.teardown(db_ctx.conn, uc1.WS_ENFORCE);

    try store.upsertCreditState(db_ctx.conn, std.testing.allocator, uc1.WS_ENFORCE, .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .remaining_credit_cents = 0,
        .exhausted_at = 123,
    }, 123);

    try std.testing.expectError(error.CreditExhausted, enforceExecutionAllowed(db_ctx.conn, std.testing.allocator, uc1.WS_ENFORCE, .free));

    const scale_credit = try enforceExecutionAllowed(db_ctx.conn, std.testing.allocator, uc1.WS_ENFORCE, .scale);
    defer std.testing.allocator.free(scale_credit.currency);
    try std.testing.expectEqual(@as(i64, 0), scale_credit.remaining_credit_cents);
}

test "deductCompletedRuntimeUsage debits free-plan credit once per completed run" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try provisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, uc1.WS_DEDUCT, "test");

    const first = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        uc1.WS_DEDUCT,
        "0195b4ba-8d3a-7f13-8abc-aa0000000093",
        1,
        42,
        "worker",
    );
    defer std.testing.allocator.free(first.currency);
    try std.testing.expectEqual(@as(i64, 42), first.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS - 42, first.remaining_credit_cents);

    const second = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        uc1.WS_DEDUCT,
        "0195b4ba-8d3a-7f13-8abc-aa0000000093",
        1,
        42,
        "worker",
    );
    defer std.testing.allocator.free(second.currency);
    try std.testing.expectEqual(@as(i64, 42), second.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS - 42, second.remaining_credit_cents);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT COUNT(*)::BIGINT
            \\FROM workspace_credit_audit
            \\WHERE workspace_id = $1
            \\  AND event_type = 'CREDIT_DEDUCTED'
        , .{uc1.WS_DEDUCT}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        const cnt = try row.get(i64, 0);
        try std.testing.expectEqual(@as(i64, 1), cnt);
    }
}

test "deductCompletedRuntimeUsage clamps to zero and records exhaustion" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_CLAMP);
    defer uc1.teardown(db_ctx.conn, uc1.WS_CLAMP);

    try store.upsertCreditState(db_ctx.conn, std.testing.allocator, uc1.WS_CLAMP, .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = 995,
        .remaining_credit_cents = 5,
        .exhausted_at = null,
    }, 123);

    const credit_result = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        uc1.WS_CLAMP,
        "0195b4ba-8d3a-7f13-8abc-aa0000000094",
        1,
        30,
        "worker",
    );
    defer std.testing.allocator.free(credit_result.currency);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit_result.consumed_credit_cents);
    try std.testing.expectEqual(@as(i64, 0), credit_result.remaining_credit_cents);
    try std.testing.expect(credit_result.exhausted_at != null);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT COUNT(*)::BIGINT
            \\FROM workspace_credit_audit
            \\WHERE workspace_id = $1
            \\  AND event_type = 'CREDIT_EXHAUSTED'
        , .{uc1.WS_CLAMP}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        const cnt = try row.get(i64, 0);
        try std.testing.expectEqual(@as(i64, 1), cnt);
    }
}
