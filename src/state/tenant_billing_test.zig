// Tests for src/state/tenant_billing.zig — M11_005.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const tenant_billing = @import("tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

test "PlanTier parse: case-insensitive; unknown defaults to free" {
    try std.testing.expectEqual(tenant_billing.PlanTier.scale, tenant_billing.PlanTier.parse("scale"));
    try std.testing.expectEqual(tenant_billing.PlanTier.scale, tenant_billing.PlanTier.parse("SCALE"));
    try std.testing.expectEqual(tenant_billing.PlanTier.free, tenant_billing.PlanTier.parse("free"));
    try std.testing.expectEqual(tenant_billing.PlanTier.free, tenant_billing.PlanTier.parse("bogus"));
}

test "PlanTier.label round-trips" {
    try std.testing.expectEqualStrings("free", tenant_billing.PlanTier.free.label());
    try std.testing.expectEqualStrings("scale", tenant_billing.PlanTier.scale.label());
}

test "runtimeUsageCostCents: linear in agent_seconds, overflow-safe" {
    try std.testing.expectEqual(@as(i64, 0), tenant_billing.runtimeUsageCostCents(0));
    try std.testing.expectEqual(@as(i64, 30), tenant_billing.runtimeUsageCostCents(30));
}

test "provision inserts one row and replay is a no-op" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_PROVISION);
    defer uc1.teardown(db_ctx.conn, uc1.WS_PROVISION);

    try tenant_billing.provisionFreeDefault(db_ctx.conn, uc1.TENANT_ID);
    // Second call must be idempotent.
    try tenant_billing.provisionFreeDefault(db_ctx.conn, uc1.TENANT_ID);

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqualStrings("free", row.plan_tier);
    try std.testing.expectEqualStrings("free_default", row.plan_sku);
    try std.testing.expectEqual(@as(i64, 1000), row.balance_cents);
    try std.testing.expectEqualStrings("bootstrap_free_grant", row.grant_source);
}

test "debit decrements atomically; 0-row UPDATE returns CreditExhausted" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.provisionFreeDefault(db_ctx.conn, uc1.TENANT_ID);

    const after = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 5);
    try std.testing.expectEqual(@as(i64, 995), after.balance_cents);

    // Exhaust: try to debit more than remaining.
    try std.testing.expectError(error.CreditExhausted, tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 10_000));

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 995), row.balance_cents);
}

test "debit on missing tenant returns CreditExhausted (0-row UPDATE)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const unknown = "0195b4ba-8d3a-7f13-8abc-aaffffffff01";
    try std.testing.expectError(error.CreditExhausted, tenant_billing.debit(db_ctx.conn, unknown, 1));
}

test "resolveTenantFromWorkspace returns the owning tenant" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_ENFORCE);
    defer uc1.teardown(db_ctx.conn, uc1.WS_ENFORCE);

    const tid = try tenant_billing.resolveTenantFromWorkspace(db_ctx.conn, ALLOC, uc1.WS_ENFORCE);
    defer ALLOC.free(tid);
    try std.testing.expectEqualStrings(uc1.TENANT_ID, tid);
}

test "error mapping: CreditExhausted → UZ-BILLING-005" {
    try std.testing.expectEqualStrings("UZ-BILLING-005", tenant_billing.errorCode(error.CreditExhausted).?);
    try std.testing.expect(tenant_billing.errorMessage(error.CreditExhausted) != null);
}
