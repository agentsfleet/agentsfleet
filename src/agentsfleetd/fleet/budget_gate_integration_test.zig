//! End-to-end integration tests for the per-fleet budget gate.
//!
//! `budget_integration_test.zig` proves the spend query and the budget read.
//! These prove the GATE: an over-budget fleet driven through the real lease path
//! (`POST /v1/runners/me/leases` → `runBilling`) is refused with a terminal
//! `gate_blocked` + `budget_breach` row and is never charged the receive fee,
//! while an under-budget fleet leases exactly as before.
//!
//! Reuses the lifecycle harness (TestHarness + live Postgres + Redis), the same
//! way `event_lifecycle_reclaim_integration_test.zig` does.
//!
//! Note on spend: every charge is zero until `FREE_TRIAL_END_MS`
//! (2026-08-01T00:00:00Z), so a fleet driven through the HTTP path today accrues
//! no `credit_deducted_nanos` of its own. These tests therefore seed the spend
//! directly, which is what the gate actually reads — the gate's decision is
//! independent of how the telemetry rows came to exist.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const life = @import("event_lifecycle_integration_test.zig");
const base = @import("../db/test_fixtures.zig");
const event_rows = @import("event_rows.zig");
const store = @import("../state/fleet_telemetry_store.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const contract = @import("contract");

const ALLOC = std.testing.allocator;

const FLEET_OVER = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01";
const FLEET_UNDER = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e02";
const FIXTURE_MODEL = "budget-gate-test-model";
const HOUR_MS: i64 = 60 * 60 * 1000;

/// `daily_dollars: 1.0` → a 1_000_000_000-nano ceiling. No monthly ceiling, so
/// only the rolling-day window can refuse.
const CONFIG_DAILY_ONE_DOLLAR =
    \\{"name":"budget-gate","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
;

/// Exactly the $1.00 ceiling, in nanos. `covers` refuses at equality, so this
/// spend is over budget — the boundary the unit tests pin.
const SPEND_AT_CEILING_NANOS: i64 = 1_000_000_000;

fn seedSpend(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8, nanos: i64, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = life.WORKSPACE_ID,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = nanos,
        .recorded_at = recorded_at,
    });
}

fn teardownSpend(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{life.WORKSPACE_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn deleteStream(h: anytype, fleet_id: []const u8) void {
    var key_buf: [128]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id}) catch return;
    var resp = h.queue.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

/// Telemetry rows of one charge type for one event — the receive-debit probe.
fn chargeRowCount(conn: *pg.Conn, event_id: []const u8, charge_type: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.fleet_execution_telemetry
        \\WHERE event_id = $1 AND charge_type = $2
    , .{ event_id, charge_type }));
    defer q.deinit();
    const row = (try q.next()).?;
    return try row.get(i64, 0);
}

// ── The pre-run gate refuses, and never charges ─────────────────────────────

test "integration: an over-budget fleet is refused the lease: gate_blocked + budget_breach + XACK" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_OVER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_OVER, "budget-over", CONFIG_DAILY_ONE_DOLLAR, "9");
    // Spend exactly at the ceiling, one hour ago — inside the rolling day.
    try seedSpend(conn, FLEET_OVER, "evt-budget-prior-spend", SPEND_AT_CEILING_NANOS, clock.nowMillis() - HOUR_MS);

    const event_id = try life.publishEvent(h, FLEET_OVER);
    defer h.queue.alloc.free(event_id);

    // No lease issues: the fleet has spent its own allowance for the day.
    try std.testing.expect(!try life.pollLease(h));
    try life.expectRow(conn, FLEET_OVER, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BUDGET_BREACH);
    // Terminal, so the stream entry is acknowledged — not left pending.
    try std.testing.expectEqual(@as(i64, 0), try life.pendingCount(h, FLEET_OVER));
}

test "integration: a budget refusal is taken before the receive debit, so the event is never charged" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_OVER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_OVER, "budget-over", CONFIG_DAILY_ONE_DOLLAR, "9");
    try seedSpend(conn, FLEET_OVER, "evt-budget-prior-spend", SPEND_AT_CEILING_NANOS, clock.nowMillis() - HOUR_MS);

    const event_id = try life.publishEvent(h, FLEET_OVER);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(!try life.pollLease(h));

    // Invariant 2: the gate runs BEFORE `debitReceive`, so a refused event has
    // no `receive` telemetry row. Charging a fleet for an event it was refused
    // would be the budget quietly costing money to enforce.
    try std.testing.expectEqual(@as(i64, 0), try chargeRowCount(conn, event_id, "receive"));
}

// ── The positive control: the gate does not touch a healthy fleet ───────────

test "integration: an under-budget fleet leases exactly as before" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_UNDER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_UNDER, "budget-under", CONFIG_DAILY_ONE_DOLLAR, "a");
    // One nano short of the ceiling: `covers` admits strictly below.
    try seedSpend(conn, FLEET_UNDER, "evt-budget-under", SPEND_AT_CEILING_NANOS - 1, clock.nowMillis() - HOUR_MS);

    const event_id = try life.publishEvent(h, FLEET_UNDER);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(try life.pollLease(h));
    try life.expectRow(conn, FLEET_UNDER, event_id, event_rows.STATUS_RECEIVED, "");
}

test "integration: a fleet whose spend is outside the rolling day window leases" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_UNDER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_UNDER, "budget-under", CONFIG_DAILY_ONE_DOLLAR, "a");
    // Well over the daily ceiling, but 25 hours ago — the window has rolled past
    // it. With no `monthly_dollars` declared, nothing else can refuse.
    try seedSpend(conn, FLEET_UNDER, "evt-budget-yesterday", SPEND_AT_CEILING_NANOS * 10, clock.nowMillis() - 25 * HOUR_MS);

    const event_id = try life.publishEvent(h, FLEET_UNDER);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(try life.pollLease(h));
    try life.expectRow(conn, FLEET_UNDER, event_id, event_rows.STATUS_RECEIVED, "");
}

// ── The mid-run kill's label reaches the durable record ─────────────────────

test "integration: a budget-killed run persists failure_label=budget_breach on the event row" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_UNDER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_UNDER, "budget-under", CONFIG_DAILY_ONE_DOLLAR, "a");
    const event_id = try life.publishEvent(h, FLEET_UNDER);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try life.pollLease(h)); // the run starts under budget

    // Mid-run, `/renew` refuses (UZ-RUN-015) and the runner reports this result.
    // The class the control plane named must land verbatim in `failure_label` —
    // this is what lets an operator answer "did my budget hold?" from the row.
    event_rows.markTerminal(h.pool, FLEET_UNDER, event_id, .{
        .exit_ok = false,
        .failure = contract.execution_result.FailureClass.budget_breach,
    }, 1234);

    try life.expectRow(conn, FLEET_UNDER, event_id, event_rows.STATUS_FLEET_ERROR, event_rows.LABEL_BUDGET_BREACH);
}

test "integration: a credit-exhausted kill still reports renewal_terminate, not budget_breach" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteStream(h, FLEET_UNDER);
    defer teardownSpend(conn);

    try life.seedFleetWithConfig(conn, FLEET_UNDER, "budget-under", CONFIG_DAILY_ONE_DOLLAR, "a");
    const event_id = try life.publishEvent(h, FLEET_UNDER);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try life.pollLease(h));

    // The whole point of the new label is that it is DISTINCT. A tenant-credit
    // stop must not be mistaken for the fleet author's own ceiling.
    event_rows.markTerminal(h.pool, FLEET_UNDER, event_id, .{
        .exit_ok = false,
        .failure = contract.execution_result.FailureClass.renewal_terminate,
    }, 1234);

    try life.expectRow(conn, FLEET_UNDER, event_id, event_rows.STATUS_FLEET_ERROR, "renewal_terminate");
}
