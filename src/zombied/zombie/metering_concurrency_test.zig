// Concurrency tests for src/zombied/zombie/metering.zig — proves the
// credit-pool debit path holds under contention: no lost writes when many
// events drain the same tenant, and atomic exhaustion (exactly one debit
// marks the gate, the rest see .exhausted).
//
// Each worker thread calls the public metering entrypoint, which acquires
// its own pooled connection; the pg.Pool (size 4) serializes acquisition so
// 100 threads contend through a handful of real connections. The debit SQL
// is a conditional UPDATE (balance >= nanos) inside a transaction, so the
// DB enforces the no-lost-update invariant — these tests assert the outcome
// distribution, not the locking mechanism.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_RECEIVE_RACE = "0195b4ba-8d3a-7f13-8abc-aa0800000001";
const WS_STAGE_EXHAUST_RACE = "0195b4ba-8d3a-7f13-8abc-aa0800000002";

const N_WORKERS = 100;

// Per-worker context: a distinct event_id so each debit writes its own
// telemetry row (UNIQUE event_id+charge_type). Outcome captured by-ref.
const Job = struct {
    pool: *@import("pg").Pool,
    workspace_id: []const u8,
    event_id: [40]u8,
    event_len: usize,
    outcome: metering.DebitOutcome = .{ .db_error = {} },

    fn ctx(self: *const Job) metering.PreflightContext {
        return .{
            .workspace_id = self.workspace_id,
            .zombie_id = "zombie-conc-test",
            .event_id = self.event_id[0..self.event_len],
            .posture = .self_managed,
            .model = "any-model-self-managed",
        };
    }
};

fn runReceive(job: *Job) void {
    job.outcome = metering.debitReceive(job.pool, ALLOC, uc1.TENANT_ID, job.ctx(), .stop);
}

fn runStage(job: *Job) void {
    job.outcome = metering.debitStage(job.pool, ALLOC, uc1.TENANT_ID, job.ctx(), .stop);
}

// event_id pattern: aa19 segment + a per-index suffix, zero-padded, unique.
fn fillEventId(job: *Job, prefix: u8, idx: usize) void {
    const written = std.fmt.bufPrint(
        &job.event_id,
        "0195b4ba-8d3a-7f13-8abc-aa19{x:0>2}{d:0>6}",
        .{ prefix, idx },
    ) catch |e| std.debug.panic("event_id bufPrint: {s}", .{@errorName(e)});
    job.event_len = written.len;
}

test "should drain every concurrent receive debit without lost writes" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_RECEIVE_RACE);
    defer uc1.teardown(db_ctx.conn, WS_RECEIVE_RACE);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_RECEIVE_RACE}) catch {};

    // Ample balance so no receive debit can exhaust — receive charges
    // EVENT_NANOS (0) per event, so the balance never moves, but every call
    // must still succeed and write exactly one telemetry row.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, tenant_billing.STARTER_CREDIT_NANOS, "test_recv_race");
    const initial = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(initial.grant_source));

    var jobs: [N_WORKERS]Job = undefined;
    var threads: [N_WORKERS]std.Thread = undefined;
    for (&jobs, 0..) |*j, i| {
        j.* = .{ .pool = db_ctx.pool, .workspace_id = WS_RECEIVE_RACE, .event_id = undefined, .event_len = 0 };
        fillEventId(j, 0xC0, i);
    }
    for (&threads, &jobs) |*t, *j| t.* = try std.Thread.spawn(.{}, runReceive, .{j});
    for (&threads) |t| t.join();

    // Every worker must have deducted EVENT_NANOS (0); none exhausted.
    for (&jobs) |*j| {
        switch (j.outcome) {
            .deducted => |c| try std.testing.expectEqual(tenant_billing.EVENT_NANOS, c),
            else => return error.TestExpectedEqual,
        }
    }

    // Balance unchanged (receive = 0); no phantom drains, no lost updates.
    const final = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(final.grant_source));
    try std.testing.expectEqual(initial.balance_nanos, final.balance_nanos);

    // Exactly N receive telemetry rows — one per distinct event_id, no losses.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1 AND charge_type = 'receive'
    , .{WS_RECEIVE_RACE}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, N_WORKERS), try r.get(i64, 0));
}

test "should mark exhaustion exactly once when concurrent stage debits outrun the balance" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_STAGE_EXHAUST_RACE);
    defer uc1.teardown(db_ctx.conn, WS_STAGE_EXHAUST_RACE);
    defer _ = db_ctx.conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{WS_STAGE_EXHAUST_RACE}) catch {};

    // self-managed stage charge is STAGE_SELF_MANAGED_NANOS post-trial. Fund
    // exactly one stage debit so the first winner drains it and the rest
    // exhaust. While the trial is open the charge is 0 (no debit ever
    // exhausts), so this race can only be exercised post-trial.
    try tenant_billing.provision(db_ctx.conn, uc1.TENANT_ID, tenant_billing.STAGE_SELF_MANAGED_NANOS, "test_exhaust_race");
    const trial_active = blk: {
        const b = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
        defer ALLOC.free(@constCast(b.grant_source));
        break :blk b.free_trial_active;
    };
    if (trial_active) return error.SkipZigTest;

    var jobs: [N_WORKERS]Job = undefined;
    var threads: [N_WORKERS]std.Thread = undefined;
    for (&jobs, 0..) |*j, i| {
        j.* = .{ .pool = db_ctx.pool, .workspace_id = WS_STAGE_EXHAUST_RACE, .event_id = undefined, .event_len = 0 };
        fillEventId(j, 0xC1, i);
    }
    for (&threads, &jobs) |*t, *j| t.* = try std.Thread.spawn(.{}, runStage, .{j});
    for (&threads) |t| t.join();

    // Exactly one .deducted (the winner), the rest .exhausted. db_error is a
    // fail (would mean a transaction fault, not a contention outcome).
    var deducted: usize = 0;
    var exhausted: usize = 0;
    for (&jobs) |*j| {
        switch (j.outcome) {
            .deducted => deducted += 1,
            .exhausted => exhausted += 1,
            else => return error.TestExpectedEqual,
        }
    }
    try std.testing.expectEqual(@as(usize, 1), deducted);
    try std.testing.expectEqual(@as(usize, N_WORKERS - 1), exhausted);

    // Balance fully drained and exhausted gate stamped exactly once.
    const final = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, uc1.TENANT_ID)).?;
    defer ALLOC.free(@constCast(final.grant_source));
    try std.testing.expectEqual(@as(i64, 0), final.balance_nanos);
    try std.testing.expect(final.exhausted_at_ms != null);
}
