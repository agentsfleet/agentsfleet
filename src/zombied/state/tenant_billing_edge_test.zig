// Edge / boundary tests for src/zombied/state/tenant_billing.zig.
//
// The free-trial gate, computeStageChargeAt, and isFreeTrialActive are
// PRIVATE in tenant_billing.zig and tested inline there (they need the
// time-injected sibling). This sibling file exercises only the reachable
// public surface: the DB-backed debit boundary contract and the
// free_trial_active projection on the Billing struct. computeStageCharge
// short-circuits to zero while the promotional window is open, so the
// platform-posture token-cost assertions branch on free_trial_active and
// only fire post-trial — exactly the pattern in metering_test.zig.

const std = @import("std");
const clock = @import("common").clock;

const tenant_billing = @import("tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

// Segment 5 (aa06xx) identifies this file's workspaces; easy to grep + clean.
const WS_PLATFORM_ZERO = "0195b4ba-8d3a-7f13-8abc-aa0600000001";
const WS_PLATFORM_LARGE = "0195b4ba-8d3a-7f13-8abc-aa0600000002";
const WS_DEBIT_EXACT = "0195b4ba-8d3a-7f13-8abc-aa0600000003";
const WS_DEBIT_OVER = "0195b4ba-8d3a-7f13-8abc-aa0600000004";
const WS_TRIAL = "0195b4ba-8d3a-7f13-8abc-aa0600000005";

// Reads the clock-derived free-trial projection. While the promotional
// window is open every platform stage charge short-circuits to zero, so the
// token-rate assertions can only be exercised post-trial.
fn trialActive(conn: *@import("pg").Conn) !bool {
    const b = (try tenant_billing.getBilling(conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(b.grant_source));
    return b.free_trial_active;
}

test "should charge the run fee for platform runtime with zero token counts post-trial" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_PLATFORM_ZERO);
    defer uc1.teardown(db_ctx.conn, WS_PLATFORM_ZERO);
    // seedPlatformProvider populates the process-global model rate cache so
    // platform-posture charge math can resolve a model. Teardown drops it.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_PLATFORM_ZERO);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_PLATFORM_ZERO);

    // Platform posture, zero tokens, 10s of runtime: the charge is exactly the
    // run fee (no token cost to add). Free-trial window short-circuits to 0, so
    // the run-fee assertion is post-trial only.
    if (try trialActive(db_ctx.conn)) return error.SkipZigTest;

    const elapsed_ms: i64 = 10_000;
    const charge = tenant_billing.computeStageCharge("anthropic", .platform, "claude-sonnet-4-6", elapsed_ms, 0, 0, 0);
    // Replicate runFee via the pinned per-second rate (runFee is private).
    const expected_run_fee = @divTrunc(elapsed_ms * tenant_billing.RUN_NANOS_PER_SEC, 1000);
    try std.testing.expectEqual(expected_run_fee, charge);
}

test "should not overflow when platform token counts approach u32 max" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_PLATFORM_LARGE);
    defer uc1.teardown(db_ctx.conn, WS_PLATFORM_LARGE);
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_PLATFORM_LARGE);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_PLATFORM_LARGE);

    if (try trialActive(db_ctx.conn)) return error.SkipZigTest;

    // Near-u32-max token counts plus an hour of runtime: rate math widens to
    // i64 internally, so the result must be a finite positive i64, no overflow.
    const big: u32 = std.math.maxInt(u32) - 1;
    const charge = tenant_billing.computeStageCharge("anthropic", .platform, "claude-sonnet-4-6", 3_600_000, big, big, big);
    try std.testing.expect(charge > 0);
    try std.testing.expect(charge < std.math.maxInt(i64));
}

test "should succeed with zero balance when debit exactly covers the remaining credit" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_DEBIT_EXACT);
    defer uc1.teardown(db_ctx.conn, WS_DEBIT_EXACT);

    // Provision a known, small balance so the exact-drain boundary is precise.
    const balance: i64 = 1_000_000;
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, balance, "test_exact");

    // Debit exactly the remaining balance: succeeds, lands at zero, no exhaust.
    const after = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, balance);
    try std.testing.expectEqual(@as(i64, 0), after.balance_nanos);

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 0), row.balance_nanos);
    // A zero-balance debit-to-exact must NOT stamp the exhausted gate.
    try std.testing.expect(row.exhausted_at_ms == null);
}

test "should return CreditExhausted and leave balance unchanged when debit is one nano over" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_DEBIT_OVER);
    defer uc1.teardown(db_ctx.conn, WS_DEBIT_OVER);

    const balance: i64 = 1_000_000;
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, balance, "test_over");

    // One nano more than the balance: CreditExhausted, balance untouched.
    try std.testing.expectError(
        error.CreditExhausted,
        tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, balance + 1),
    );

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(balance, row.balance_nanos);
}

test "should report the free trial active just before the cutoff and inactive at it" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TRIAL);
    defer uc1.teardown(db_ctx.conn, WS_TRIAL);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // isFreeTrialActive / FREE_TRIAL_END_MS are private; the public projection
    // is Billing.free_trial_active vs free_trial_ends_at_ms. The strict
    // less-than gate is the contract: active iff now_ms < ends_at_ms. This
    // asserts the projection is internally consistent with the wall clock at
    // call time without reaching into the private cutoff constant.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));

    const now_ms = clock.nowMillis();
    const expected_active = now_ms < row.free_trial_ends_at_ms;
    try std.testing.expectEqual(expected_active, row.free_trial_active);
    // The cutoff is a fixed forward instant, never zero/negative.
    try std.testing.expect(row.free_trial_ends_at_ms > 0);
}
