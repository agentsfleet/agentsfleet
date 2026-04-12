// Tests for src/zombie/metering.zig — M15_001.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const workspace_credit = @import("../state/workspace_credit.zig");
const workspace_credit_store = @import("../state/workspace_credit_store.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

// Segment 5 (aa05*) identifies M15_001 metering workspaces.
const WS_SCALE_EXEMPT = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_DEDUCTED = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_EXHAUSTED = "0195b4ba-8d3a-7f13-8abc-aa0500000003";
const WS_REPLAY = "0195b4ba-8d3a-7f13-8abc-aa0500000004";

// T1 (spec dim 1.2) — DeductionResult variants compile and round-trip cleanly.
test "DeductionResult variants compile and pattern-match" {
    const r1: metering.DeductionResult = .{ .deducted = 7 };
    const r2: metering.DeductionResult = .{ .exempt = {} };
    const r3: metering.DeductionResult = .{ .exhausted = 0 };
    const r4: metering.DeductionResult = .{ .db_error = {} };

    try std.testing.expectEqual(@as(i64, 7), switch (r1) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });
    try std.testing.expect(switch (r2) {
        .exempt => true,
        else => false,
    });
    try std.testing.expect(switch (r3) {
        .exhausted => |c| c == 0,
        else => false,
    });
    try std.testing.expect(switch (r4) {
        .db_error => true,
        else => false,
    });
}

// T2 (spec dim 1.3) — Scale-plan workspaces short-circuit before any DB call.
// The `undefined` conn proves we never touch the DB on the exempt path.
test "deductZombieUsage exempt on scale plan does not touch DB" {
    const conn: *pg.Conn = undefined;
    const result = metering.deductZombieUsage(conn, ALLOC, .{
        .zombie_id = "z-1",
        .workspace_id = WS_SCALE_EXEMPT,
        .event_id = "e-1",
        .agent_seconds = 30,
        .token_count = 0,
    }, .scale);
    try std.testing.expect(switch (result) {
        .exempt => true,
        else => false,
    });
}

// T3 (spec dim 1.4 / 2.1) — Free-plan deduction writes CREDIT_DEDUCTED audit row.
test "deductZombieUsage on free plan writes CREDIT_DEDUCTED audit row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_DEDUCTED);
    defer uc1.teardown(db_ctx.conn, WS_DEDUCTED);

    try workspace_credit.provisionWorkspaceCredit(db_ctx.conn, ALLOC, WS_DEDUCTED, "test");

    const result = metering.deductZombieUsage(db_ctx.conn, ALLOC, .{
        .zombie_id = "zombie-m15-dedupe",
        .workspace_id = WS_DEDUCTED,
        .event_id = "0195b4ba-8d3a-7f13-8abc-aa050000ee01",
        .agent_seconds = 60,
        .token_count = 100,
    }, .free);

    try std.testing.expectEqual(@as(i64, 60), switch (result) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT
        \\FROM workspace_credit_audit
        \\WHERE workspace_id = $1 AND event_type = 'CREDIT_DEDUCTED' AND reason = 'runtime_completed'
    , .{WS_DEDUCTED}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// T4 (spec dim 1.4) — Exhausted credit still writes a zero-delta audit row.
test "deductZombieUsage exhausted writes zero-delta audit and returns .exhausted" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_EXHAUSTED);
    defer uc1.teardown(db_ctx.conn, WS_EXHAUSTED);

    try workspace_credit_store.upsertCreditState(db_ctx.conn, ALLOC, WS_EXHAUSTED, .{
        .currency = workspace_credit.CREDIT_CURRENCY,
        .initial_credit_cents = workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        .remaining_credit_cents = 0,
        .exhausted_at = 123,
    }, 123);

    const result = metering.deductZombieUsage(db_ctx.conn, ALLOC, .{
        .zombie_id = "zombie-m15-exhaust",
        .workspace_id = WS_EXHAUSTED,
        .event_id = "0195b4ba-8d3a-7f13-8abc-aa050000ee02",
        .agent_seconds = 30,
        .token_count = 0,
    }, .free);

    try std.testing.expect(switch (result) {
        .exhausted => |c| c == 0,
        else => false,
    });

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT delta_credit_cents, remaining_credit_cents
        \\FROM workspace_credit_audit
        \\WHERE workspace_id = $1 AND event_type = 'CREDIT_DEDUCTED'
    , .{WS_EXHAUSTED}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 1));
}

// T5 (spec dim 2.2) — DB failure returns .db_error; event loop still XACKs
// because recordZombieDelivery returns void (no error propagation).
// Injection: workspace_id that doesn't exist → FK violation on the INSERT in
// provisionWorkspaceCredit inside getOrProvisionWorkspaceCredit.
test "deductZombieUsage returns .db_error on DB write failure" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Deliberately NOT seeded — workspace_id has no matching row in `workspaces`.
    const WS_GHOST = "0195b4ba-8d3a-7f13-8abc-aa0500000099";

    const result = metering.deductZombieUsage(db_ctx.conn, ALLOC, .{
        .zombie_id = "zombie-m15-ghost",
        .workspace_id = WS_GHOST,
        .event_id = "0195b4ba-8d3a-7f13-8abc-aa050000ee99",
        .agent_seconds = 30,
        .token_count = 0,
    }, .free);

    try std.testing.expect(switch (result) {
        .db_error => true,
        else => false,
    });
}

// T6 (spec dim 2.3) — Replay of same event_id deducts 0 cents.
test "deductZombieUsage is idempotent on crash-recovery replay" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_REPLAY);
    defer uc1.teardown(db_ctx.conn, WS_REPLAY);

    try workspace_credit.provisionWorkspaceCredit(db_ctx.conn, ALLOC, WS_REPLAY, "test");

    const usage = metering.ExecutionUsage{
        .zombie_id = "zombie-m15-replay",
        .workspace_id = WS_REPLAY,
        .event_id = "0195b4ba-8d3a-7f13-8abc-aa050000ee03",
        .agent_seconds = 25,
        .token_count = 0,
    };

    const first = metering.deductZombieUsage(db_ctx.conn, ALLOC, usage, .free);
    try std.testing.expectEqual(@as(i64, 25), switch (first) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });

    const second = metering.deductZombieUsage(db_ctx.conn, ALLOC, usage, .free);
    try std.testing.expectEqual(@as(i64, 0), switch (second) {
        .deducted => |c| c,
        else => @as(i64, -2),
    });

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT
        \\FROM workspace_credit_audit
        \\WHERE workspace_id = $1 AND event_type = 'CREDIT_DEDUCTED'
    , .{WS_REPLAY}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}
