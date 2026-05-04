// Tests for src/zombie/metering.zig — two-debit credit-pool path.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const balance_policy = @import("../config/balance_policy.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_GATE_PASS = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_GATE_BLOCK = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_RECEIVE_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0500000003";
const WS_STAGE_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0500000004";
const WS_RECEIVE_EXHAUST = "0195b4ba-8d3a-7f13-8abc-aa0500000005";

fn makeCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .zombie_id = "zombie-test",
        .event_id = event_id,
        .posture = .byok,
        .model = "any-model-byok-doesnt-need-cache",
    };
}

test "DebitOutcome variants compile and pattern-match" {
    const r1: metering.DebitOutcome = .{ .deducted = 7 };
    const r2: metering.DebitOutcome = .{ .exhausted = {} };
    const r3: metering.DebitOutcome = .{ .missing_tenant_billing = {} };
    const r4: metering.DebitOutcome = .{ .db_error = {} };

    try std.testing.expectEqual(@as(i64, 7), switch (r1) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });
    switch (r2) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }
    switch (r3) {
        .missing_tenant_billing => {},
        else => return error.TestExpectedEqual,
    }
    switch (r4) {
        .db_error => {},
        else => return error.TestExpectedEqual,
    }
}

test "balanceCoversEstimate: returns true under non-stop policies regardless of balance" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_GATE_PASS);
    defer uc1.teardown(db_ctx.conn, WS_GATE_PASS);

    // Provision at 0¢ — balance is empty, but non-stop policies must let
    // the event through.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, "free", "free.v1", 0, "test_continue");

    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .byok,
        "any-model",
        .@"continue",
    ));
}

test "balanceCoversEstimate: blocks when stop policy AND balance below est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_GATE_BLOCK);
    defer uc1.teardown(db_ctx.conn, WS_GATE_BLOCK);

    // BYOK: receive=0¢, stage=1¢ overhead (no token math under BYOK).
    // est_total = 1¢. Balance = 0¢ < 1¢ → blocked.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, "free", "free.v1", 0, "test_block");

    try std.testing.expect(!metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .byok,
        "any-model",
        .stop,
    ));
}

test "balanceCoversEstimate: passes when stop policy AND balance covers est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // 500¢ starter grant covers a 1¢ BYOK event.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .byok,
        "any-model",
        .stop,
    ));
}

test "debitReceive BYOK: 0¢ charge writes telemetry row, balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer uc1.teardown(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_RECEIVE_DEBIT}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a01";
    const result = metering.debitReceive(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        makeCtx(WS_RECEIVE_DEBIT, event_id),
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(tenant_billing.RECEIVE_BYOK_CENTS, c),
        else => return error.TestExpectedEqual,
    }

    // Balance unchanged (BYOK receive = 0¢).
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 500), row.balance_cents);

    // Telemetry row must exist with charge_type='receive'.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, credit_deducted_cents
        \\FROM zombie_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("receive", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("byok", try r.get([]const u8, 1));
    try std.testing.expectEqual(@as(i64, 0), try r.get(i64, 2));
}

test "debitStage BYOK: 1¢ flat overhead drains balance and writes stage telemetry" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_STAGE_DEBIT);
    defer uc1.teardown(db_ctx.conn, WS_STAGE_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_STAGE_DEBIT}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a02";
    const result = metering.debitStage(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        makeCtx(WS_STAGE_DEBIT, event_id),
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(tenant_billing.STAGE_OVERHEAD_BYOK_CENTS, c),
        else => return error.TestExpectedEqual,
    }

    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 499), row.balance_cents); // 500 - 1¢ stage overhead

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, token_count_input, token_count_output, wall_ms
        \\FROM zombie_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("stage", try r.get([]const u8, 0));
    try std.testing.expectEqual(@as(?i64, null), try r.get(?i64, 1)); // tokens NULL pre-execution
    try std.testing.expectEqual(@as(?i64, null), try r.get(?i64, 2));
    try std.testing.expectEqual(@as(?i64, null), try r.get(?i64, 3));
}

test "debit on insufficient balance returns .exhausted; no rows written, balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_RECEIVE_EXHAUST);
    defer uc1.teardown(db_ctx.conn, WS_RECEIVE_EXHAUST);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_RECEIVE_EXHAUST}) catch {};

    // Provision at 0¢ — stage debit (1¢ overhead) must exhaust.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, "free", "free.v1", 0, "test_exhaust");

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a03";
    const result = metering.debitStage(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        makeCtx(WS_RECEIVE_EXHAUST, event_id),
        .stop,
    );
    switch (result) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }

    // Balance unchanged at 0¢.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.plan_tier));
    defer ALLOC.free(@constCast(row.plan_sku));
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(@as(i64, 0), row.balance_cents);
    // exhausted_at must be stamped on the first failed debit so the stop
    // gate stays closed for subsequent events until a top-up clears it.
    try std.testing.expect(row.exhausted_at_ms != null);

    // No telemetry row written — the transaction rolled back.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 0), try r.get(i64, 0));
}

test "recordStageActuals: updates stage row tokens and wall_ms, leaves cents untouched" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const ws = "0195b4ba-8d3a-7f13-8abc-aa0500000007";
    try uc1.seed(db_ctx.conn, ws);
    defer uc1.teardown(db_ctx.conn, ws);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{ws}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // Land a stage debit first so we have a row to update.
    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a05";
    const ctx = makeCtx(ws, event_id);
    switch (metering.debitStage(db_ctx.pool, ALLOC, uc1.TENANT_ID, ctx, .stop)) {
        .deducted => {},
        else => return error.TestExpectedEqual,
    }

    metering.recordStageActuals(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        ctx,
        100, // input tokens
        540, // output tokens
        12_500, // wall_ms
        std.time.milliTimestamp(),
    );

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT credit_deducted_cents, token_count_input, token_count_output, wall_ms
        \\FROM zombie_execution_telemetry WHERE event_id = $1 AND charge_type = 'stage'
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    // credit_deducted_cents: pre-execution estimate is the charge of record.
    try std.testing.expectEqual(tenant_billing.STAGE_OVERHEAD_BYOK_CENTS, try r.get(i64, 0));
    try std.testing.expectEqual(@as(?i64, 100), try r.get(?i64, 1));
    try std.testing.expectEqual(@as(?i64, 540), try r.get(?i64, 2));
    try std.testing.expectEqual(@as(?i64, 12_500), try r.get(?i64, 3));
}

test "telemetry insert is idempotent: same event_id+charge_type replayed inserts 1 row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const ws = "0195b4ba-8d3a-7f13-8abc-aa0500000008";
    try uc1.seed(db_ctx.conn, ws);
    defer uc1.teardown(db_ctx.conn, ws);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{ws}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a06";
    const ctx = makeCtx(ws, event_id);

    _ = metering.debitReceive(db_ctx.pool, ALLOC, uc1.TENANT_ID, ctx, .stop);
    // Replay: the second INSERT must hit ON CONFLICT DO NOTHING.
    _ = metering.debitReceive(db_ctx.pool, ALLOC, uc1.TENANT_ID, ctx, .stop);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try r.get(i64, 0));
}
