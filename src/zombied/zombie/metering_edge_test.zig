// Edge / boundary tests for src/zombied/zombie/metering.zig — the
// balanceCoversEstimate stop-gate boundary (the one-shot stage debit retired
// with incremental metering; the gate is what remains at issue).
//
// computeStageCharge short-circuits to zero while the promotional free-trial
// window is open (now_ms < FREE_TRIAL_END_MS), so platform token-cost and
// stop-gate-block assertions branch on free_trial_active and only fire
// post-trial — the same honest-in-both-eras shape used by metering_test.zig.

const std = @import("std");

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_GATE_EXACT = "0195b4ba-8d3a-7f13-8abc-aa0700000002";
const WS_GATE_UNDER = "0195b4ba-8d3a-7f13-8abc-aa0700000003";

fn trialActive(conn: *@import("pg").Conn) !bool {
    const b = (try tenant_billing.getBilling(conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(b.grant_source));
    return b.free_trial_active;
}

test "should pass the stop gate for self_managed at zero balance (no upfront charge)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_GATE_EXACT);
    defer uc1.teardown(db_ctx.conn, WS_GATE_EXACT);

    // Under the run-fee model the self_managed issue-time estimate is zero —
    // receive (EVENT_NANOS=0) plus runFee(0)=0; tokens land on the user's own
    // provider. So a freshly provisioned zero balance still clears the gate; the
    // run fee is metered per renewal and refused at the next renewal once credit
    // runs out.
    const est_total = tenant_billing.computeReceiveCharge(.self_managed) +
        tenant_billing.computeStageCharge(
            "self-managed-test",
            .self_managed,
            "any-model-self-managed",
            0, // elapsed_ms
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    try std.testing.expectEqual(@as(i64, 0), est_total);
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, est_total, "test_gate_exact");

    // balance == est_total == 0 → gate passes (>= comparison, not strict).
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model-self-managed",
        .stop,
    ));
}

test "should block the stop gate when balance is one nano below the estimate post-trial" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_GATE_UNDER);
    defer uc1.teardown(db_ctx.conn, WS_GATE_UNDER);

    // Platform posture so the issue-time estimate (the token floor) is non-zero
    // and the refuse boundary is reachable; self_managed carries no upfront
    // charge under the run-fee model. The cache must hold the platform model.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_GATE_UNDER);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_GATE_UNDER);

    const est_total = tenant_billing.computeReceiveCharge(.platform) +
        tenant_billing.computeStageCharge(
            "anthropic",
            .platform,
            "claude-sonnet-4-6",
            0, // elapsed_ms: issue-time estimate carries no run fee
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    // Provision the exact estimate first: the billing row must exist for the
    // trial probe, and est_total is non-negative in both eras (0 while the
    // trial zeroes the charge, the token floor after).
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, est_total, "test_gate_under");

    // While the trial window is open est_total == 0, so "one nano below" is a
    // negative balance the gate can't reach honestly — skip until post-trial.
    // The block path is real; the fixture just can't drive it through the
    // public charge while the gate is closed.
    if (try trialActive(db_ctx.conn)) return error.SkipZigTest;

    // Post-trial: drop exactly one nano below the estimate so the stop gate
    // must refuse.
    _ = try tenant_billing.debit(db_ctx.conn, uc1.TENANT_ID, 1);

    try std.testing.expect(!metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .platform,
        "anthropic",
        "claude-sonnet-4-6",
        .stop,
    ));
}
