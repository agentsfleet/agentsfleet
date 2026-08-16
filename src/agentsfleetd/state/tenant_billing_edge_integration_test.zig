// Edge / boundary tests for src/agentsfleetd/state/tenant_billing.zig.
//
// The connection-free rate math is tested inline in tenant_billing_rates.zig,
// where the private catalogue-free branch is reachable. This file exercises the
// DB-backed surface: the debit boundary rules, the starter grant as the whole
// free allowance, and the platform pricing paths. Nothing here arranges a clock
// — pricing reads the catalogue and the posture, so every assertion runs
// unconditionally.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const tenant_billing = @import("tenant_billing.zig");
const billing_rates = @import("tenant_billing_rates.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// Per-suite tenant (fa06 block, matching the aa06 workspace segment): keeps
// this suite's grants/resets off every other suite's balance assertions.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-fa0600000000";

fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedTenantById(conn, TENANT_ID, "tenant-billing-edge-suite");
    try base.seedWorkspaceWithTenant(conn, workspace_id, TENANT_ID);
}

fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, TENANT_ID);
}

// Segment 5 (aa06xx) identifies this file's workspaces; easy to grep + clean.
const WS_PLATFORM_ZERO = "0195b4ba-8d3a-7f13-8abc-aa0600000001";
const WS_PLATFORM_LARGE = "0195b4ba-8d3a-7f13-8abc-aa0600000002";
const WS_STARTER_GRANT = "0195b4ba-8d3a-7f13-8abc-aa0600000005";

// Suite-private (provider, model) pair with real token rates for the overflow
// test — the fixture platform pair's zero rates would multiply the near-max
// token counts by nothing and prove nothing about widening.
const RATE_PROVIDER = "billing-edge-provider";
const RATE_MODEL = "billing-edge-model";
const RATE_MODEL_UID = "0195b4ba-8d3a-7f13-8abc-fa0600000061";
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

test "integration: should charge the run fee for platform runtime with zero token counts" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_PLATFORM_ZERO);
    defer teardown(db_ctx.conn, WS_PLATFORM_ZERO);
    // seedPlatformProvider populates the process-global model rate cache so
    // platform-posture charge math can resolve a model. Teardown drops it.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_PLATFORM_ZERO);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_PLATFORM_ZERO);

    // Platform posture, zero tokens, 10s of runtime: the charge is exactly the
    // run fee. The fixture platform pair's token rates are zero by design, so
    // pricing THAT pair keeps the expected value the run fee alone.
    const elapsed_ms: i64 = 10_000;
    const charge = try billing_rates.computeStageCharge(db_ctx.conn, base.TEST_PROVIDER_NAME, .platform, base.TEST_PLATFORM_MODEL, elapsed_ms, 0, 0, 0);
    // Replicate runFee via the pinned per-second rate (runFee is private).
    const expected_run_fee = @divTrunc(elapsed_ms * tenant_billing.RUN_NANOS_PER_SEC, 1000);
    try std.testing.expectEqual(expected_run_fee, charge);
}

test "integration: should not overflow when platform token counts approach u32 max" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_PLATFORM_LARGE);
    defer teardown(db_ctx.conn, WS_PLATFORM_LARGE);
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_PLATFORM_LARGE);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_PLATFORM_LARGE);
    // Real (non-zero) token rates so the near-max counts actually exercise the
    // widening — zero rates would multiply by nothing and prove nothing.
    try seedRatedModel(db_ctx.conn);
    defer teardownRatedModel(db_ctx.conn);

    // Near-u32-max token counts plus an hour of runtime: rate math widens to
    // i64 internally, so the result must be a finite positive i64, no overflow.
    const big: u32 = std.math.maxInt(u32) - 1;
    const charge = try billing_rates.computeStageCharge(db_ctx.conn, RATE_PROVIDER, .platform, RATE_MODEL, 3_600_000, big, big, big);
    try std.testing.expect(charge > 0);
    try std.testing.expect(charge < std.math.maxInt(i64));
}

test "integration: should refuse to price an uncatalogued model with error.ModelNotPriced" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Deliberately NO model seeding: the point is a (provider, model) pair the
    // catalogue has no row for — the state an admin DELETE of a non-default row
    // leaves behind for any tenant still naming that model. This used to panic
    // and abort the replica; it must be an error the caller's posture absorbs.
    //
    // Reachable with no arrangement at all: the catalogue lookup is the only
    // thing standing between the caller and the refusal.
    try std.testing.expectError(error.ModelNotPriced, billing_rates.computeStageCharge(
        db_ctx.conn,
        "no-such-provider",
        .platform,
        "no-such-model",
        0,
        0,
        0,
        0,
    ));
}

test "integration: the starter grant is the whole free allowance a fresh tenant gets" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_STARTER_GRANT);
    defer teardown(db_ctx.conn, WS_STARTER_GRANT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    // What replaced the promotional window. A fresh tenant's free usage is a
    // balance, not a date: positive on arrival, and bounded by that number
    // rather than by a clock nobody set. Its exhaustion mark starts clear —
    // `balance_exhausted_at` is now the only signal that free usage ran out.
    const row = (try tenant_billing.getBilling(db_ctx.conn, TENANT_ID)).?;

    try std.testing.expect(row.balance_nanos > 0);
    try std.testing.expect(row.exhausted_at_ms == null);
}

/// Abort the transaction so every subsequent statement on this connection
/// errors. Same mechanism `budget_integration_test` uses to prove its own
/// fail-open branch is reachable.
fn poisonTransaction(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
    try std.testing.expectError(error.PG, conn.exec("SELECT 1/0", .{}));
}

fn healTransaction(conn: *pg.Conn) void {
    _ = conn.exec("ROLLBACK", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "integration: a wallet read on a faulted connection errors rather than answering" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Both consumers of `getBilling` route a read failure somewhere specific —
    // the billing handler answers `internalDbUnavailable`, and the pre-claim
    // metering gate fails OPEN so a database blip cannot refuse every lease. Both
    // of those branches are `catch` arms, and a `catch` arm nothing can reach is
    // dead code wearing a safety label. This proves the arm is live: the read
    // genuinely errors on a faulted connection rather than returning null, which
    // would read as "this tenant has no wallet" and take the wrong path entirely.
    //
    // The exact error variant is driver-drain dependent and immaterial — both
    // call sites swallow any of them with `catch`.
    try poisonTransaction(db_ctx.conn);
    defer healTransaction(db_ctx.conn);

    if (tenant_billing.getBilling(db_ctx.conn, TENANT_ID)) |_| {
        return error.TestExpectedWalletReadToFailOnPoisonedTxn;
    } else |_| {}
}
