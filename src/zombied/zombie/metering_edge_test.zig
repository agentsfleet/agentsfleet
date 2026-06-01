// Edge / boundary tests for src/zombied/zombie/metering.zig — platform
// posture debit path and the balanceCoversEstimate gate boundary.
//
// computeStageCharge short-circuits to zero while the promotional free-trial
// window is open (now_ms < FREE_TRIAL_END_MS), so platform token-cost and
// stop-gate-block assertions branch on free_trial_active and only fire
// post-trial — the same honest-in-both-eras shape used by metering_test.zig.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_PLATFORM_STAGE = "0195b4ba-8d3a-7f13-8abc-aa0700000001";
const WS_GATE_EXACT = "0195b4ba-8d3a-7f13-8abc-aa0700000002";
const WS_GATE_UNDER = "0195b4ba-8d3a-7f13-8abc-aa0700000003";

fn platformCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .zombie_id = "zombie-edge-test",
        .event_id = event_id,
        .posture = .platform,
        .provider = "anthropic",
        .model = "claude-sonnet-4-6",
    };
}

fn trialActive(conn: *@import("pg").Conn) !bool {
    const b = (try tenant_billing.getBilling(conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(b.grant_source));
    return b.free_trial_active;
}

test "should drain the platform stage charge and write stage telemetry post-trial" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_PLATFORM_STAGE);
    defer uc1.teardown(db_ctx.conn, WS_PLATFORM_STAGE);
    // Populates the process-global model rate cache; platform posture needs it.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_PLATFORM_STAGE);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_PLATFORM_STAGE);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_PLATFORM_STAGE}) catch {};

    // expected = STAGE_PLATFORM_NANOS + per-token cost at floor tokens. While
    // the trial is open the whole charge is 0; branch so the test stays honest.
    const expected = blk: {
        if (try trialActive(db_ctx.conn)) break :blk @as(i64, 0);
        break :blk tenant_billing.computeStageCharge(
            "anthropic",
            .platform,
            "claude-sonnet-4-6",
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    };
    // Post-trial the flat platform fee must be present in the charge.
    if (!(try trialActive(db_ctx.conn))) {
        try std.testing.expect(expected >= tenant_billing.STAGE_PLATFORM_NANOS);
    }

    const before = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(before.grant_source));

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000b01";
    const result = metering.debitStage(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        platformCtx(WS_PLATFORM_STAGE, event_id),
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(expected, c),
        else => return error.TestExpectedEqual,
    }

    const after = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(after.grant_source));
    try std.testing.expectEqual(before.balance_nanos - expected, after.balance_nanos);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, credit_deducted_nanos
        \\FROM zombie_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("stage", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("platform", try r.get([]const u8, 1));
    try std.testing.expectEqual(expected, try r.get(i64, 2));
}

test "should pass the stop gate when balance exactly equals the conservative estimate" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_GATE_EXACT);
    defer uc1.teardown(db_ctx.conn, WS_GATE_EXACT);

    // est_total = receive (EVENT_NANOS=0) + stage at floor tokens. Compute it
    // through the same public path the gate uses so the boundary is exact.
    const est_total = tenant_billing.computeReceiveCharge(.self_managed) +
        tenant_billing.computeStageCharge(
            "self-managed-test",
            .self_managed,
            "any-model-self-managed",
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, est_total, "test_gate_exact");

    // balance == est_total → gate passes (>= comparison, not strict).
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

    const est_total = tenant_billing.computeReceiveCharge(.self_managed) +
        tenant_billing.computeStageCharge(
            "self-managed-test",
            .self_managed,
            "any-model-self-managed",
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    // Provision the exact estimate first: the billing row must exist for the
    // trial probe, and est_total is non-negative in both eras (0 while the
    // trial zeroes the charge, the flat fee after).
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
        .self_managed,
        "self-managed-test",
        "any-model-self-managed",
        .stop,
    ));
}
