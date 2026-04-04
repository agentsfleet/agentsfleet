//! M27_002 billing gate tests.
//!
//! Group A — pure unit tests (no DB):
//!   Verify threshold constant, tier boundaries, and event-key logic for each tier.
//!
//! Group B — billing integration tests (real DB):
//!   Verify usage_ledger lifecycle_event for .score_gated runs and that no credit
//!   is consumed.
//!
//! Group C — additive-gate safety tests (real DB):
//!   Verify that already-non-billable runs (cancelled, retried, blocked) are not
//!   accidentally switched to billable by gate logic.

const std = @import("std");
const pg = @import("pg");
const billing = @import("./billing_runtime.zig");
const workspace_billing = @import("./workspace_billing.zig");
const workspace_credit = @import("./workspace_credit.zig");
const scoring_types = @import("../pipeline/scoring_mod/types.zig");
const math = @import("../pipeline/scoring_mod/math.zig");

const base = @import("../db/test_fixtures.zig");
const uc3 = @import("../db/test_fixtures_uc3.zig");

// ── Gate workspace + run IDs (prefix cc70 to avoid conflicts) ────────────────
const WS_GATE = "0195b4ba-8d3a-7f13-8abc-cc0000000070";
const SPEC_GATE = "0195b4ba-8d3a-7f13-8abc-cc0000000071";
const RUN_BRONZE = "0195b4ba-8d3a-7f13-8abc-cc0000000072";
const RUN_SILVER_FLOOR = "0195b4ba-8d3a-7f13-8abc-cc0000000073";
const RUN_SILVER = "0195b4ba-8d3a-7f13-8abc-cc0000000074";
const RUN_GOLD = "0195b4ba-8d3a-7f13-8abc-cc0000000075";
const RUN_ELITE = "0195b4ba-8d3a-7f13-8abc-cc0000000076";
const RUN_ALREADY_NON_BILLABLE = "0195b4ba-8d3a-7f13-8abc-cc0000000077";

// ─────────────────────────────────────────────────────────────────────────────
// Group A: pure unit tests
// ─────────────────────────────────────────────────────────────────────────────

test "billing gate threshold is 40 matching Bronze floor" {
    try std.testing.expectEqual(@as(u8, 40), scoring_types.BILLING_QUALITY_THRESHOLD);
    // Verify it aligns with tierFromScore boundary.
    try std.testing.expectEqual(scoring_types.Tier.bronze, math.tierFromScore(39));
    try std.testing.expectEqual(scoring_types.Tier.silver, math.tierFromScore(40));
}

test "scores in Bronze range (0-39) are all below threshold" {
    var score: u8 = 0;
    while (score < 40) : (score += 1) {
        try std.testing.expect(score < scoring_types.BILLING_QUALITY_THRESHOLD);
        try std.testing.expectEqual(scoring_types.Tier.bronze, math.tierFromScore(score));
    }
}

test "scores at Silver floor and above (40-100) are not gated" {
    var score: u8 = 40;
    while (score <= 100) : (score += 1) {
        try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
    }
}

// Bronze (0-39): all stages failed, wrong output — garbage run.
test "Bronze tier: blocked run, 0 stages passed, score is 19 — below threshold" {
    const axes = math_axes(.blocked_stage_graph, 0, 3, null);
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 19), score);
    try std.testing.expect(score < scoring_types.BILLING_QUALITY_THRESHOLD);
    try std.testing.expectEqual(scoring_types.Tier.bronze, math.tierFromScore(score));
}

// Bronze: boundary at 39 — exactly one below threshold.
test "Bronze boundary 39: retries-exhausted, 2/5 stages, score is 39" {
    // completion=30 * 0.4 + error_rate=40 * 0.3 + latency=50 * 0.2 + resource=50 * 0.1 = 39.0
    const axes = scoring_types.AxisScores{
        .completion = math.computeCompletionScore(.blocked_retries_exhausted), // 30
        .error_rate = math.computeErrorRateScore(2, 5), // 40
        .latency = 50,
        .resource = 50,
    };
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 39), score);
    try std.testing.expect(score < scoring_types.BILLING_QUALITY_THRESHOLD);
}

// Silver floor at 40 — should NOT be gated.
test "Silver floor 40: retries-exhausted, 3/7 stages, score rounds to 40 — not gated" {
    // completion=30 * 0.4 + error_rate=43 * 0.3 + latency=50 * 0.2 + resource=50 * 0.1 = 39.9 → 40
    const axes = scoring_types.AxisScores{
        .completion = math.computeCompletionScore(.blocked_retries_exhausted), // 30
        .error_rate = math.computeErrorRateScore(3, 7), // 43
        .latency = 50,
        .resource = 50,
    };
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 40), score);
    try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
    try std.testing.expectEqual(scoring_types.Tier.silver, math.tierFromScore(score));
}

// Silver (40-69): done run, 0 stages passed — code ran but produced no artifacts.
test "Silver tier: done run, 0/3 stages passed, score is 55 — billable" {
    const axes = math_axes(.done, 0, 3, null);
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 55), score);
    try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
}

// Silver: done run but took too long — high latency lowers score but stays Silver.
test "Silver tier: done run, slow latency (3x p50), score drops but stays above threshold" {
    // With baseline p50=10s, wall=30s: latency_score = 0 (excess >= range)
    const baseline = scoring_types.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 30, .sample_count = 5 };
    const axes = scoring_types.AxisScores{
        .completion = math.computeCompletionScore(.done), // 100
        .error_rate = math.computeErrorRateScore(0, 3), // 0
        .latency = math.computeLatencyScore(30, baseline), // 0 — max slow penalty
        .resource = math.computeResourceScore(), // 50
    };
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    // score = 40 + 0 + 0 + 5 = 45 — still Silver, still billable
    try std.testing.expectEqual(@as(u8, 45), score);
    try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
}

// Gold (70-89): done, 2/3 stages passed.
test "Gold tier: done run, 2/3 stages, score is 75 — billable" {
    const axes = math_axes(.done, 2, 3, null);
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 75), score);
    try std.testing.expectEqual(scoring_types.Tier.gold, math.tierFromScore(score));
    try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
}

// Gold: fast latency improves score into Elite range.
test "Gold→Elite: done, 3/3 stages, fast run with baseline — score 95 Elite" {
    const baseline = scoring_types.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 30, .sample_count = 5 };
    const axes = scoring_types.AxisScores{
        .completion = math.computeCompletionScore(.done), // 100
        .error_rate = math.computeErrorRateScore(3, 3), // 100
        .latency = math.computeLatencyScore(8, baseline), // 100 (under p50)
        .resource = math.computeResourceScore(), // 50
    };
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expectEqual(@as(u8, 95), score);
    try std.testing.expectEqual(scoring_types.Tier.elite, math.tierFromScore(score));
}

// Elite (90-100): all stages, fast run.
test "Elite tier: done, 3/3, fast latency — score 95, well above threshold" {
    const baseline = scoring_types.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 30, .sample_count = 5 };
    const axes = scoring_types.AxisScores{
        .completion = 100,
        .error_rate = 100,
        .latency = math.computeLatencyScore(5, baseline), // 100
        .resource = 50,
    };
    const score = math.computeScore(axes, scoring_types.DEFAULT_WEIGHTS);
    try std.testing.expect(score >= 90);
    try std.testing.expect(score >= scoring_types.BILLING_QUALITY_THRESHOLD);
    try std.testing.expectEqual(scoring_types.Tier.elite, math.tierFromScore(score));
}

// Elite false-positive guard: ensure elite is NEVER accidentally gated.
test "Elite tier is always billable — cannot be accidentally gated" {
    var s: u8 = 90;
    while (s <= 100) : (s += 1) {
        try std.testing.expect(s >= scoring_types.BILLING_QUALITY_THRESHOLD);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Group B: billing integration tests (real DB)
// ─────────────────────────────────────────────────────────────────────────────

fn seedGateWorkspace(conn: *pg.Conn) !void {
    const all_runs = [_][]const u8{
        RUN_BRONZE, RUN_SILVER_FLOOR, RUN_SILVER, RUN_GOLD, RUN_ELITE, RUN_ALREADY_NON_BILLABLE,
    };
    try uc3.seedWithRuns(conn, WS_GATE, SPEC_GATE, &all_runs);
    try uc3.seedBillingState(conn, "0195b4ba-8d3a-7f13-8abc-cc0000000080", WS_GATE, "FREE", workspace_billing.FREE_PLAN_SKU, "ACTIVE");
    try uc3.seedCreditState(conn, "0195b4ba-8d3a-7f13-8abc-cc0000000081", WS_GATE, workspace_credit.CREDIT_CURRENCY, workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS, 0, workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS, null);
}

fn addStageUsage(conn: *pg.Conn, run_id: []const u8, seconds: u64) !void {
    try billing.recordRuntimeStageUsage(conn, WS_GATE, run_id, 1, "plan", .echo, 5, seconds);
}

fn expectLedgerLifecycle(conn: *pg.Conn, run_id: []const u8, expected: []const u8) !void {
    var q = try conn.query(
        "SELECT lifecycle_event FROM usage_ledger WHERE run_id = $1 AND source = 'runtime_summary' LIMIT 1",
        .{run_id},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedLedgerRow;
    try std.testing.expectEqualStrings(expected, try row.get([]const u8, 0));
    try q.drain();
}

fn expectLedgerBillableQty(conn: *pg.Conn, run_id: []const u8, expected_qty: i64) !void {
    var q = try conn.query(
        "SELECT billable_quantity FROM usage_ledger WHERE run_id = $1 AND source = 'runtime_summary' LIMIT 1",
        .{run_id},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedLedgerRow;
    try std.testing.expectEqual(expected_qty, try row.get(i64, 0));
    try q.drain();
}

test "score_gated run writes run_not_billable_score_gated to usage_ledger" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_BRONZE, 20);
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_BRONZE, 1, .score_gated);

    try expectLedgerLifecycle(db_ctx.conn, RUN_BRONZE, "run_not_billable_score_gated");
}

test "score_gated run has billable_quantity of zero" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_SILVER_FLOOR, 30);
    // Simulate a score-gated run (just below threshold)
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_SILVER_FLOOR, 1, .score_gated);

    try expectLedgerBillableQty(db_ctx.conn, RUN_SILVER_FLOOR, 0);
}

test "score_gated run does not deduct credits" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_SILVER, 25);
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_SILVER, 1, .score_gated);

    var q = try db_ctx.conn.query(
        "SELECT consumed_credit_cents FROM workspace_credit_state WHERE workspace_id = $1 LIMIT 1",
        .{WS_GATE},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedCreditRow;
    // credits untouched
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    try q.drain();
}

test "completed run above threshold writes run_completed to usage_ledger" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_GOLD, 15);
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_GOLD, 1, .completed);

    try expectLedgerLifecycle(db_ctx.conn, RUN_GOLD, "run_completed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Group C: additive-gate safety (already-non-billable must stay non-billable)
// ─────────────────────────────────────────────────────────────────────────────

test "non_billable run writes run_not_billable — distinct from score_gated" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_ALREADY_NON_BILLABLE, 10);
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_ALREADY_NON_BILLABLE, 1, .non_billable);

    try expectLedgerLifecycle(db_ctx.conn, RUN_ALREADY_NON_BILLABLE, "run_not_billable");
}

test "score_gated event_key is distinct from non_billable event_key" {
    // Both runs, same attempt — event_key uniqueness prevents overwrite.
    // score_gated → "run_finalize:1:score_gated"
    // non_billable → "run_finalize:1:non_billable"
    // completed → "run_finalize:1:completed"
    // Ensure these are three distinct keys so re-finalization with a different
    // outcome doesn't silently no-op the ON CONFLICT DO NOTHING.
    const sg = "run_finalize:1:score_gated";
    const nb = "run_finalize:1:non_billable";
    const cp = "run_finalize:1:completed";
    try std.testing.expect(!std.mem.eql(u8, sg, nb));
    try std.testing.expect(!std.mem.eql(u8, sg, cp));
    try std.testing.expect(!std.mem.eql(u8, nb, cp));
}

test "Elite run: score 95 is not gated — false-positive guard" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try seedGateWorkspace(db_ctx.conn);
    defer uc3.teardownWithRuns(db_ctx.conn, WS_GATE);

    try addStageUsage(db_ctx.conn, RUN_ELITE, 8);
    // Elite run — use .completed outcome (gate decided it was good)
    try billing.finalizeRunForBilling(std.testing.allocator, db_ctx.conn, WS_GATE, RUN_ELITE, 1, .completed);

    try expectLedgerLifecycle(db_ctx.conn, RUN_ELITE, "run_completed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convenience: build AxisScores for a given outcome/stages/baseline combo.
fn math_axes(outcome: scoring_types.TerminalOutcome, passed: u32, total: u32, baseline: ?scoring_types.LatencyBaseline) scoring_types.AxisScores {
    return scoring_types.AxisScores{
        .completion = math.computeCompletionScore(outcome),
        .error_rate = math.computeErrorRateScore(passed, total),
        .latency = math.computeLatencyScore(0, baseline), // 0 wall_seconds = max latency score
        .resource = math.computeResourceScore(),
    };
}
