// Idempotency / invariant tests for the credit-pool debit path.
//
// Covers two replay-safety invariants: provision is a no-op on a tenant that
// already has a billing row (ON CONFLICT DO NOTHING), and a zero-nanos
// receive debit still commits a telemetry row while leaving the balance
// untouched (computeReceiveCharge is EVENT_NANOS=0 under both postures).

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_PROVISION_REPLAY = "0195b4ba-8d3a-7f13-8abc-aa0900000001";
const WS_ZERO_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0900000002";

fn selfManagedCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .fleet_id = "fleet-idem-test",
        .event_id = event_id,
        .posture = .self_managed,
        .provider = "self-managed-test",
        .model = "any-model-self-managed",
    };
}

test "should keep the first balance when provision is replayed on the same tenant" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_PROVISION_REPLAY);
    defer uc1.teardown(db_ctx.conn, WS_PROVISION_REPLAY);

    const balance1: i64 = 3_000_000;
    const balance2: i64 = 9_000_000;

    // First provision lands balance1 + source1.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, balance1, "source_first");
    // Second provision with a different balance/source must be a silent no-op
    // (ON CONFLICT DO NOTHING) — the row stays as the first call left it.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, balance2, "source_second");

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(balance1, row.balance_nanos);
    try std.testing.expectEqualStrings("source_first", row.grant_source);
}

test "should commit a telemetry row with zero deducted nanos and leave balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_ZERO_DEBIT);
    defer uc1.teardown(db_ctx.conn, WS_ZERO_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{WS_ZERO_DEBIT}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);
    const before = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(before.grant_source));

    // computeReceiveCharge is EVENT_NANOS (0) under every posture, so the
    // debit branch is skipped (nanos == 0) yet the telemetry INSERT still
    // fires inside the committed transaction.
    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000d01";
    const result = metering.debitReceive(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        selfManagedCtx(WS_ZERO_DEBIT, event_id),
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(@as(i64, 0), c),
        else => return error.TestExpectedEqual,
    }

    // Balance untouched — no nanos drained on a zero charge.
    const after = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(after.grant_source));
    try std.testing.expectEqual(before.balance_nanos, after.balance_nanos);

    // Telemetry row present with credit_deducted_nanos = 0.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, credit_deducted_nanos
        \\FROM core.fleet_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("receive", try r.get([]const u8, 0));
    try std.testing.expectEqual(@as(i64, 0), try r.get(i64, 1));
}
