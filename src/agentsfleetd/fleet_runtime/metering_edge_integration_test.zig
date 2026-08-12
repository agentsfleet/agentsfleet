// Edge / boundary tests for src/agentsfleetd/fleet_runtime/metering.zig — the
// balanceCoversEstimate stop-gate boundary (the one-shot stage debit retired
// with incremental metering; the gate is what remains at issue).
//
// Pricing depends on no clock, so the platform token-cost and stop-gate-block
// assertions run unconditionally — no wall-clock date can put them to sleep.

const std = @import("std");
const pg = @import("pg");

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const billing_rates = @import("../state/tenant_billing_rates.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const clock = @import("common").clock;
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// Per-suite tenant (fa07 block, matching the aa07 workspace segment): keeps
// this suite's grants/resets off every other suite's balance assertions.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-fa0700000000";

const WS_GATE_EXACT = "0195b4ba-8d3a-7f13-8abc-aa0700000002";
const WS_GATE_UNDER = "0195b4ba-8d3a-7f13-8abc-aa0700000003";

fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedTenantById(conn, TENANT_ID, "metering-edge-suite");
    try base.seedWorkspaceWithTenant(conn, workspace_id, TENANT_ID);
}

fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, TENANT_ID);
}

// Suite-private (provider, model) pair with real token rates: the one-nano-
// below refusal needs a non-zero estimate floor, which neither the zero-rated
// fixture platform pair nor an uncatalogued pair (fails OPEN) can provide.
const RATE_PROVIDER = "metering-edge-provider";
const RATE_MODEL = "metering-edge-model";
const RATE_MODEL_UID = "0195b4ba-8d3a-7f13-8abc-fa0700000061";
const RATE_INPUT_NANOS_PER_MTOK: i64 = 3_000_000;
const RATE_CACHED_NANOS_PER_MTOK: i64 = 300_000;
const RATE_OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000;
const RATE_MODEL_CAP_TOKENS: i64 = 200_000;

fn seedRatedModel(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO UPDATE SET
        \\   input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
        \\   output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
        \\   updated_at = EXCLUDED.updated_at
    , .{ RATE_MODEL_UID, RATE_MODEL, RATE_PROVIDER, RATE_MODEL_CAP_TOKENS, RATE_INPUT_NANOS_PER_MTOK, RATE_CACHED_NANOS_PER_MTOK, RATE_OUTPUT_NANOS_PER_MTOK, clock.nowMillis() });
}

fn teardownRatedModel(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1 AND model_id = $2", .{ RATE_PROVIDER, RATE_MODEL }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    model_rate_cache.clear();
}

test "integration: should pass the stop gate for self_managed at zero balance (no upfront charge)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_EXACT);
    defer teardown(db_ctx.conn, WS_GATE_EXACT);

    // Under the run-fee model the self_managed issue-time estimate is zero —
    // receive (EVENT_NANOS=0) plus runFee(0)=0; tokens land on the user's own
    // provider. So a freshly provisioned zero balance still clears the gate; the
    // run fee is metered per renewal and refused at the next renewal once credit
    // runs out. The zero comes from the self_managed branch genuinely.
    const est_total = tenant_billing.computeReceiveCharge(.self_managed) +
        try billing_rates.computeStageCharge(
            db_ctx.conn,
            "self-managed-test",
            .self_managed,
            "any-model-self-managed",
            0, // elapsed_ms
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    try std.testing.expectEqual(@as(i64, 0), est_total);
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, est_total, "test_gate_exact");

    // balance == est_total == 0 → gate passes (>= comparison, not strict).
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model-self-managed",
        .stop,
    ));
}

test "integration: should block the stop gate when balance is one nano below the estimate" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_UNDER);
    defer teardown(db_ctx.conn, WS_GATE_UNDER);

    // Platform posture so the issue-time estimate (the token floor) is non-zero
    // and the refuse boundary is reachable; self_managed carries no upfront
    // charge under the run-fee model. Priced against this suite's rated pair —
    // the fixture pair's zero token rates would leave the floor at 0.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_GATE_UNDER);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_GATE_UNDER);
    try seedRatedModel(db_ctx.conn);
    defer teardownRatedModel(db_ctx.conn);

    const est_total = tenant_billing.computeReceiveCharge(.platform) +
        try billing_rates.computeStageCharge(
            db_ctx.conn,
            RATE_PROVIDER,
            .platform,
            RATE_MODEL,
            0, // elapsed_ms: issue-time estimate carries no run fee
            tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
    try std.testing.expect(est_total > 0);
    // Provision the exact estimate. seedPlatformProvider granted the starter
    // balance and provision is idempotent — reset so est_total actually lands.
    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, est_total, "test_gate_under");

    // Drop exactly one nano below the estimate so the stop gate must refuse.
    _ = try tenant_billing.debit(db_ctx.conn, TENANT_ID, 1);

    try std.testing.expect(!metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .platform,
        RATE_PROVIDER,
        RATE_MODEL,
        .stop,
    ));
}
