// DB-backed tests for src/agentsfleetd/fleet_runtime/metering.zig — the
// two-debit credit-pool path against live PostgreSQL. Split from
// metering_test.zig: these opt into the database, so they belong to the
// integration root (`integration_tests.zig`), where the canonical lanes
// actually provide one; parked in the unit root they executed in no lane.
//
// Pricing depends on no clock, so the refusal asserts unconditionally.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const clock = @import("common").clock;
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// Suite-private (provider, model) pair with real token rates. The stop gate's
// refusal needs a non-zero estimate floor, and the fixture platform pair
// carries zero token rates by design — so the block test prices its own pair.
const RATE_PROVIDER = "metering-gate-provider";
const RATE_MODEL = "metering-gate-model";
const RATE_MODEL_UID = "0195b4ba-8d3a-7f13-8abc-fa0500000061";
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

// Drop the private pair and clear the process-global rate cache so no later
// suite resolves a price for a row the database no longer carries.
fn teardownRatedModel(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1 AND model_id = $2", .{ RATE_PROVIDER, RATE_MODEL }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    model_rate_cache.clear();
}

// Per-suite tenant (fa05 block, matching the aa05 workspace segment): keeps
// this suite's grants/resets off every other suite's balance assertions.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-fa0500000000";

const WS_GATE_PASS = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_GATE_BLOCK = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_RECEIVE_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0500000003";
const WS_GATE_COVERED = "0195b4ba-8d3a-7f13-8abc-aa0500000004";

/// The event envelope's creation instant. Fixed rather than `nowMillis()`:
/// every ledger row for one event must carry the SAME value (schema/710),
/// which a per-call clock read cannot guarantee.
const EVENT_CREATED_AT: i64 = 1_760_000_000_000;

/// The fleet every debit in this suite is attributed to. A real UUIDv7 with a
/// real row behind it, because `billing.usage_ledger.fleet_id` is a UUID column
/// with a foreign key onto `core.fleets` — a placeholder like "fleet-test"
/// fails the cast, the INSERT never lands, and the debit reports `db_error`.
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-fa0500000010";

fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedTenantById(conn, TENANT_ID, "metering-suite");
    try base.seedWorkspaceWithTenant(conn, workspace_id, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, workspace_id, "metering-suite-fleet", "{}", "# z");
}

fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownFleets(conn, workspace_id);
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, TENANT_ID);
}

fn makeCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .fleet_id = FLEET_ID,
        .event_id = event_id,
        .posture = .self_managed,
        .provider = "self-managed-test",
        .model = "any-model-self-managed-doesnt-need-cache",
    };
}

test "integration: balanceCoversEstimate returns true under non-stop policies regardless of balance" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_PASS);
    defer teardown(db_ctx.conn, WS_GATE_PASS);

    // Provision at 0¢ — balance is empty, but non-stop policies must let
    // the event through.
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, 0, "test_continue");

    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .@"continue",
    ));
}

test "integration: balanceCoversEstimate blocks when stop policy AND balance below est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_BLOCK);
    defer teardown(db_ctx.conn, WS_GATE_BLOCK);
    // Platform posture so est_total (the token floor) is non-zero; self_managed
    // carries no issue-time charge under the run-fee model. The floor is priced
    // against this suite's own rated pair — the fixture platform pair's zero
    // token rates would leave it 0, and an uncatalogued pair fails OPEN.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_GATE_BLOCK);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_GATE_BLOCK);
    try seedRatedModel(db_ctx.conn);
    defer teardownRatedModel(db_ctx.conn);

    // platform: receive = EVENT_NANOS (0), stage = token-floor cost (run fee is
    // 0 at issue). Balance 0 < est_total → blocked. seedPlatformProvider just
    // granted the starter balance onto this suite's tenant and provision is
    // idempotent — reset so the 0 actually lands.
    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, 0, "test_block");

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

test "integration: balanceCoversEstimate passes when stop policy AND balance covers est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_COVERED);
    defer teardown(db_ctx.conn, WS_GATE_COVERED);

    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    // STARTER_CREDIT_NANOS trivially covers a self_managed event — receive is
    // EVENT_NANOS (0) and the issue-time run fee is 0, so est_total is 0.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .stop,
    ));
}

test "integration: debitReceive self-managed EVENT_NANOS=0 charge writes telemetry row, balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer teardown(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM billing.usage_ledger WHERE workspace_id = $1", .{WS_RECEIVE_DEBIT}) catch {};

    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a01";
    const result = metering.debitReceive(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        makeCtx(WS_RECEIVE_DEBIT, event_id),
        EVENT_CREATED_AT,
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(tenant_billing.EVENT_NANOS, c),
        else => return error.TestExpectedEqual,
    }

    // Balance unchanged — receive charges EVENT_NANOS (zero) under both postures.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(tenant_billing.STARTER_CREDIT_NANOS, row.balance_nanos);

    // Telemetry row must exist with charge_type='receive'.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, credit_deducted_nanos
        \\FROM billing.usage_ledger WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("receive", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("self_managed", try r.get([]const u8, 1));
    try std.testing.expectEqual(tenant_billing.EVENT_NANOS, try r.get(i64, 2));
}

test "integration: telemetry insert is idempotent — same event_id+charge_type replayed inserts 1 row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const ws = "0195b4ba-8d3a-7f13-8abc-aa0500000008";
    try seed(db_ctx.conn, ws);
    defer teardown(db_ctx.conn, ws);
    defer _ = db_ctx.conn.exec("DELETE FROM billing.usage_ledger WHERE workspace_id = $1", .{ws}) catch {};

    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a06";
    const ctx = makeCtx(ws, event_id);

    _ = metering.debitReceive(db_ctx.pool, ALLOC, TENANT_ID, ctx, EVENT_CREATED_AT, .stop);
    // Replay: the second INSERT must hit ON CONFLICT DO NOTHING.
    _ = metering.debitReceive(db_ctx.pool, ALLOC, TENANT_ID, ctx, EVENT_CREATED_AT, .stop);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM billing.usage_ledger
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try r.get(i64, 0));
}
