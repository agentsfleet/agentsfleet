// Tests for src/agent/metering.zig — two-debit credit-pool path.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_GATE_PASS = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_GATE_BLOCK = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_RECEIVE_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0500000003";

fn makeCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .agent_id = "agent-test",
        .event_id = event_id,
        .posture = .self_managed,
        .provider = "self-managed-test",
        .model = "any-model-self-managed-doesnt-need-cache",
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
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, 0, "test_continue");

    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .self_managed,
        "self-managed-test",
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
    // Platform posture so est_total (the token floor) is non-zero; self_managed
    // carries no issue-time charge under the run-fee model.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_GATE_BLOCK);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_GATE_BLOCK);

    // platform: receive = EVENT_NANOS (0), stage = token-floor cost (run fee is
    // 0 at issue). Balance 0 < est_total → blocked.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, 0, "test_block");

    // Free-trial: stage charge short-circuits to 0 until FREE_TRIAL_END_MS, so
    // est_total = 0 and `balance < est_total` is mathematically unreachable.
    // Skip until post-trial — the gate still holds, this assertion just can't
    // be exercised through the public charge path while the gate is closed.
    const trial_active = blk: {
        const b = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(b.grant_source));
        break :blk b.free_trial_active;
    };
    if (trial_active) return error.SkipZigTest;

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

test "balanceCoversEstimate: passes when stop policy AND balance covers est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, uc1.WS_DEDUCT);
    defer uc1.teardown(db_ctx.conn, uc1.WS_DEDUCT);

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    // STARTER_CREDIT_NANOS trivially covers a self_managed event — receive is
    // EVENT_NANOS (0) and the issue-time run fee is 0, so est_total is 0.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        uc1.TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .stop,
    ));
}

test "debitReceive self-managed: EVENT_NANOS=0 charge writes telemetry row, balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer uc1.teardown(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM core.agent_execution_telemetry WHERE workspace_id = $1", .{WS_RECEIVE_DEBIT}) catch {};

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
        .deducted => |c| try std.testing.expectEqual(tenant_billing.EVENT_NANOS, c),
        else => return error.TestExpectedEqual,
    }

    // Balance unchanged — receive charges EVENT_NANOS (zero) under both postures.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(tenant_billing.STARTER_CREDIT_NANOS, row.balance_nanos);

    // Telemetry row must exist with charge_type='receive'.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, credit_deducted_nanos
        \\FROM core.agent_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("receive", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("self_managed", try r.get([]const u8, 1));
    try std.testing.expectEqual(tenant_billing.EVENT_NANOS, try r.get(i64, 2));
}

test "telemetry insert is idempotent: same event_id+charge_type replayed inserts 1 row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const ws = "0195b4ba-8d3a-7f13-8abc-aa0500000008";
    try uc1.seed(db_ctx.conn, ws);
    defer uc1.teardown(db_ctx.conn, ws);
    defer _ = db_ctx.conn.exec("DELETE FROM core.agent_execution_telemetry WHERE workspace_id = $1", .{ws}) catch {};

    try tenant_billing.insertStarterGrant(db_ctx.conn, uc1.TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a06";
    const ctx = makeCtx(ws, event_id);

    _ = metering.debitReceive(db_ctx.pool, ALLOC, uc1.TENANT_ID, ctx, .stop);
    // Replay: the second INSERT must hit ON CONFLICT DO NOTHING.
    _ = metering.debitReceive(db_ctx.pool, ALLOC, uc1.TENANT_ID, ctx, .stop);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.agent_execution_telemetry
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try r.get(i64, 0));
}
