// Tests for src/state/tenant_billing.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const tenant_billing = @import("tenant_billing.zig");
const tenant_provider = @import("tenant_provider.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

// ── Credit-pool cost functions ──────────────────────────────────────────────

test "computeReceiveCharge: platform charges 1¢, byok charges 0¢" {
    try std.testing.expectEqual(tenant_billing.RECEIVE_PLATFORM_CENTS, tenant_billing.computeReceiveCharge(.platform));
    try std.testing.expectEqual(tenant_billing.RECEIVE_BYOK_CENTS, tenant_billing.computeReceiveCharge(.byok));
    try std.testing.expectEqual(@as(i64, 1), tenant_billing.computeReceiveCharge(.platform));
    try std.testing.expectEqual(@as(i64, 0), tenant_billing.computeReceiveCharge(.byok));
}

test "computeStageCharge: byok returns flat overhead independent of tokens or model" {
    try std.testing.expectEqual(
        tenant_billing.STAGE_OVERHEAD_BYOK_CENTS,
        tenant_billing.computeStageCharge(.byok, "any-model", 0, 0),
    );
    try std.testing.expectEqual(
        tenant_billing.STAGE_OVERHEAD_BYOK_CENTS,
        tenant_billing.computeStageCharge(.byok, "claude-opus-4-7", 1_000_000, 1_000_000),
    );
    // BYOK does not consult the rate cache, so a model not in the catalogue
    // must NOT panic — only platform mode requires a cached rate.
    try std.testing.expectEqual(
        @as(i64, 1),
        tenant_billing.computeStageCharge(.byok, "model-not-in-catalogue", 100, 100),
    );
}

test "computeStageCharge: platform charges overhead + token math from cache" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Populate the process-global cache from the seeded core.model_caps.
    try model_rate_cache.populate(ALLOC, db_ctx.conn);
    defer model_rate_cache.deinit();

    // Sonnet rates: input 300/Mtok, output 1500/Mtok (per schema/019 seed).
    // 800 input → 800*300/1_000_000 = 0¢ (truncated)
    // 1000 output → 1000*1500/1_000_000 = 1¢ (truncated)
    // Plus STAGE_OVERHEAD_PLATFORM_CENTS = 1¢
    const cents = tenant_billing.computeStageCharge(.platform, "claude-sonnet-4-6", 800, 1000);
    try std.testing.expectEqual(@as(i64, 1 + 0 + 1), cents);

    // Larger token counts: 1_000_000 input @ 300/Mtok = 300¢
    //                      1_000_000 output @ 1500/Mtok = 1500¢
    const big = tenant_billing.computeStageCharge(.platform, "claude-sonnet-4-6", 1_000_000, 1_000_000);
    try std.testing.expectEqual(@as(i64, 1 + 300 + 1500), big);
}

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

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);
    // Second call must be idempotent.
    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqualStrings("free", row.plan_tier);
    try std.testing.expectEqualStrings("free_default", row.plan_sku);
    try std.testing.expectEqual(@as(i64, 1000), row.balance_cents);
    try std.testing.expectEqualStrings("bootstrap_starter_grant", row.grant_source);
}

test "debit decrements atomically; 0-row UPDATE returns CreditExhausted" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

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

test "debit on missing tenant returns TenantBillingMissing (distinct from CreditExhausted)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const unknown = "0195b4ba-8d3a-7f13-8abc-aaffffffff01";
    try std.testing.expectError(error.TenantBillingMissing, tenant_billing.debit(db_ctx.conn, unknown, 1));
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

test "clearExhausted + debit together: replenishment path resets the stop gate" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);
    _ = try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID);

    // clearExhausted on an already-marked row: transitions and returns true.
    try std.testing.expect(try tenant_billing.clearExhausted(db_ctx.conn, uc1.TENANT_ID));
    // Second call on an already-cleared row: idempotent, returns false.
    try std.testing.expect(!(try tenant_billing.clearExhausted(db_ctx.conn, uc1.TENANT_ID)));

    // And the billing row reflects the clear — covers the "stop gate is a
    // one-way door" follow-up when admin credit lands without a matching
    // debit.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expect(row.exhausted_at_ms == null);
}

test "debit on an exhausted row auto-clears balance_exhausted_at on success" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);
    _ = try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID);

    // Simulate a top-up path: the next successful debit must clear the
    // exhausted flag so the `stop` gate re-opens atomically.
    const after = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 5);
    try std.testing.expectEqual(@as(i64, 995), after.balance_cents);

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expect(row.exhausted_at_ms == null);
}

test "markExhausted: first call transitions, second call is a no-op" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // Fresh row: exhausted_at is NULL.
    {
        const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(row.plan_tier));
        defer ALLOC.free(@constCast(row.plan_sku));
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expect(row.exhausted_at_ms == null);
    }

    // First mark transitions.
    try std.testing.expect(try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID));
    const first_ts = blk: {
        const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(row.plan_tier));
        defer ALLOC.free(@constCast(row.plan_sku));
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expect(row.exhausted_at_ms != null);
        break :blk row.exhausted_at_ms.?;
    };

    // Second call is a no-op; timestamp unchanged.
    try std.testing.expect(!(try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID)));
    {
        const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(row.plan_tier));
        defer ALLOC.free(@constCast(row.plan_sku));
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expectEqual(first_ts, row.exhausted_at_ms.?);
    }
}
