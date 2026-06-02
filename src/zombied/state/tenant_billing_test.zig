// Tests for src/state/tenant_billing.zig.

const std = @import("std");

const tenant_billing = @import("tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

// ── Rate constants pinned (regression) ─────────────────────────────────────
// Mirror this with ui/packages/website/src/lib/rates.test.ts. Bumping a
// rate fails both suites and forces a conscious cross-stack update.
//
// Cross-tier role names: STARTER_CREDIT_NANOS, EVENT_NANOS, RUN_NANOS_PER_SEC
// — identical across Zig + TS, pinned by scripts/audit-cross-tier-rates.sh.

test "rates pinned: $5 starter · events free · runtime $0.0001/sec (in nanos)" {
    try std.testing.expectEqual(@as(i64, 5_000_000_000), tenant_billing.STARTER_CREDIT_NANOS);
    try std.testing.expectEqual(@as(i64, 0), tenant_billing.EVENT_NANOS);
    try std.testing.expectEqual(@as(i64, 100_000), tenant_billing.RUN_NANOS_PER_SEC);
}

// ── Credit-pool cost functions ──────────────────────────────────────────────

test "computeReceiveCharge: zero both postures" {
    try std.testing.expectEqual(@as(i64, 0), tenant_billing.computeReceiveCharge(.platform));
    try std.testing.expectEqual(@as(i64, 0), tenant_billing.computeReceiveCharge(.self_managed));
}

// `computeStageCharge` reads the system clock. While `now_ms <
// FREE_TRIAL_END_MS` (2026-08-01T00:00:00Z) it short-circuits to zero
// regardless of posture / model / tokens — the rate-math tests live
// inline in `tenant_billing.zig` so they have access to the private
// time-injected `computeStageChargeAt` for deterministic pre/mid/post
// trial coverage. This file's remaining tests don't depend on the
// clock-gated cost function.

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
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 5_000_000_000), row.balance_nanos);
    try std.testing.expectEqualStrings("bootstrap_starter_grant", row.grant_source);
}

test "debit decrements atomically; 0-row UPDATE returns CreditExhausted" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // Debit a sample charge of 1M nanos ($0.001) and check the balance lands.
    const after = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 1_000_000);
    try std.testing.expectEqual(@as(i64, 5_000_000_000 - 1_000_000), after.balance_nanos);

    // Exhaust: try to debit more than remaining (well above current balance).
    try std.testing.expectError(error.CreditExhausted, tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 6_000_000_000));

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 5_000_000_000 - 1_000_000), row.balance_nanos);
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
    const after = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 1_000_000);
    try std.testing.expectEqual(@as(i64, 5_000_000_000 - 1_000_000), after.balance_nanos);

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
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
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expect(row.exhausted_at_ms == null);
    }

    // First mark transitions.
    try std.testing.expect(try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID));
    const first_ts = blk: {
        const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expect(row.exhausted_at_ms != null);
        break :blk row.exhausted_at_ms.?;
    };

    // Second call is a no-op; timestamp unchanged.
    try std.testing.expect(!(try tenant_billing.markExhausted(db_ctx.conn, uc1.TENANT_ID)));
    {
        const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(row.grant_source));
        try std.testing.expectEqual(first_ts, row.exhausted_at_ms.?);
    }
}
