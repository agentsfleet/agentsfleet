//! Per-fleet spend ceilings — the `daily_dollars` / `monthly_dollars` a fleet
//! author declares in `TRIGGER.md`, enforced.
//!
//! Two gates consume this module, mirroring the two places the platform already
//! asks "may this run proceed?":
//!   - `service_billing.runBilling` — before the receive debit, so a refused
//!     event is never charged. Refusal writes `gate_blocked` + `budget_breach`.
//!   - `service_renew.renew` — the mid-run kill. Refusal answers `UZ-RUN-015`,
//!     the runner terminates its child, and the report lands `fleet_error` +
//!     `budget_breach`.
//!
//! The ceiling is a floor-check, not a projection: a run is admitted while
//! `spend < cap`. An admitted run may overshoot by at most one renewal window's
//! worth of tokens before its next `/renew` refuses it — bounded, not unbounded.
//!
//! Spend means credit DRAINED (`credit_deducted_nanos`), not credit metered: on
//! the slice that exhausts a wallet the remainder is forgiven, and a budget must
//! count money that actually left the pool.
//!
//! Both windows derive from ONE caller-supplied `now_ms` (RULE TIM) — this
//! module never reads the clock, so the day and month windows can never straddle
//! a tick, and every test pins time by argument.
//!
//! Fail-open posture: a DB fault yields `null`, and both callers admit the run.
//! This mirrors `metering.balanceCoversEstimate` ("Any DB failure returns true
//! (fail-open) so the gate never turns into an availability incident"). A budget
//! gate stricter than the platform's own credit gate would be an inconsistent
//! guarantee. An *unparseable stored budget* is a different thing entirely and
//! fails CLOSED — a ceiling we cannot read is not a ceiling we may ignore.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const clock = @import("common").clock;

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const ec = @import("../errors/error_registry.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const config_types = @import("../fleet_runtime/config_types.zig");
const config_helpers = @import("../fleet_runtime/config_helpers.zig");

const log = logging.scoped(.fleet_budget);

pub const FleetBudget = config_types.FleetBudget;

/// The rolling daily window. Not a calendar day: `authoring.mdx` documents
/// "Rolling 24-hour dollar ceiling", so the window slides with `now_ms`.
const ROLLING_DAY_MS: i64 = std.time.ms_per_day;

/// Both windowed sums in one statement, served by
/// `idx_fleet_execution_telemetry_workspace_id_fleet_id_recorded_at`.
/// `$3` is the rolling-day floor, `$4` the month floor; the outer predicate
/// bounds the scan to the earlier of the two.
const SELECT_SPEND_SQL =
    \\SELECT
    \\  COALESCE(SUM(credit_deducted_nanos) FILTER (WHERE recorded_at >= $3::bigint), 0)::bigint,
    \\  COALESCE(SUM(credit_deducted_nanos) FILTER (WHERE recorded_at >= $4::bigint), 0)::bigint
    \\FROM core.fleet_execution_telemetry
    \\WHERE workspace_id = $1 AND fleet_id = $2
    \\  AND recorded_at >= LEAST($3::bigint, $4::bigint)
;

/// The renew-side read: the fleet's stored budget subobject plus both windowed
/// sums, in one round trip. The lease row carries no config, so the budget is
/// read live — lowering a runaway fleet's ceiling therefore bites at its next
/// renewal tick rather than only at its next run.
const SELECT_BUDGET_AND_SPEND_SQL =
    \\SELECT
    \\  (z.config_json->'x-agentsfleet'->'budget')::text,
    \\  COALESCE((SELECT SUM(t.credit_deducted_nanos) FROM core.fleet_execution_telemetry t
    \\            WHERE t.workspace_id = $2 AND t.fleet_id = $3 AND t.recorded_at >= $4::bigint), 0)::bigint,
    \\  COALESCE((SELECT SUM(t.credit_deducted_nanos) FROM core.fleet_execution_telemetry t
    \\            WHERE t.workspace_id = $2 AND t.fleet_id = $3 AND t.recorded_at >= $5::bigint), 0)::bigint
    \\FROM core.fleets z
    \\WHERE z.id = $1::uuid
;

/// Credit drained by one fleet inside each window, in nanos.
pub const Spend = struct {
    day_nanos: i64,
    month_nanos: i64,
};

/// Why the gate refused — the caller maps this onto a log line, never onto a
/// distinct wire code (both ceilings are one `budget_breach` to the operator).
pub const Verdict = enum {
    ok,
    day_exceeded,
    month_exceeded,

    pub fn refused(self: Verdict) bool {
        return self != .ok;
    }
};

/// A budget the renew gate could not read. Distinct from "could not reach the
/// database": a malformed stored budget fails CLOSED.
pub const BudgetError = error{UnreadableBudget};

/// Dollars → nanos. The parser bounds inputs to `(0, 1000]` daily and
/// `(0, 10000]` monthly, so `dollars * 1e9` tops out around 1e13 and cannot
/// approach `i64` max; the saturating branch guards a future bound change, not
/// a live path. Rounds to nearest so `0.000000001` is one nano rather than zero
/// (a truncating cast would silently make tiny budgets free).
///
/// A non-finite or non-positive ceiling collapses to ZERO, not to "unlimited":
/// `covers` then refuses every run, because a budget we cannot make sense of
/// must not silently become permission to spend without bound.
pub fn dollarsToNanos(dollars: f64) i64 {
    if (!std.math.isFinite(dollars) or dollars <= 0) return 0;
    const scaled = @round(dollars * @as(f64, @floatFromInt(tenant_billing.NANOS_PER_USD)));
    if (scaled >= @as(f64, @floatFromInt(std.math.maxInt(i64)))) return std.math.maxInt(i64);
    return @intFromFloat(scaled);
}

/// The ceiling comparison. Admitted while strictly under; refused at equality,
/// so a fleet that has spent exactly its `daily_dollars` runs no further.
/// `monthly_dollars` is optional — absent means no monthly ceiling.
pub fn covers(budget: FleetBudget, spend: Spend) Verdict {
    if (spend.day_nanos >= dollarsToNanos(budget.daily_dollars)) return .day_exceeded;
    if (budget.monthly_dollars) |monthly| {
        if (spend.month_nanos >= dollarsToNanos(monthly)) return .month_exceeded;
    }
    return .ok;
}

/// Window floors for one `now_ms`. Split out so both queries and every test
/// derive their bounds from the same arithmetic.
fn windowFloors(now_ms: i64) struct { day: i64, month: i64 } {
    return .{
        .day = now_ms -| ROLLING_DAY_MS,
        .month = clock.startOfUtcMonthMillis(now_ms),
    };
}

/// Credit drained by `fleet_id` inside each window. `null` on ANY database
/// fault — the caller then fails open explicitly rather than by coincidence.
pub fn spendForFleet(
    pool: *pg.Pool,
    workspace_id: []const u8,
    fleet_id: []const u8,
    now_ms: i64,
) ?Spend {
    const conn = pool.acquire() catch |err| {
        log.warn("budget_acquire_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
        return null;
    };
    defer pool.release(conn);
    return spendForFleetConn(conn, workspace_id, fleet_id, now_ms) catch |err| {
        log.warn("budget_spend_query_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .err = @errorName(err) });
        return null;
    };
}

fn spendForFleetConn(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, now_ms: i64) !?Spend {
    const floors = windowFloors(now_ms);
    var q = PgQuery.from(try conn.query(SELECT_SPEND_SQL, .{ workspace_id, fleet_id, floors.day, floors.month }));
    defer q.deinit();
    const row = try q.next() orelse return Spend{ .day_nanos = 0, .month_nanos = 0 };
    return Spend{
        .day_nanos = try row.get(i64, 0),
        .month_nanos = try row.get(i64, 1),
    };
}

/// Renew-side read. Returns `null` when the fleet row is gone (the caller admits
/// — the lease is about to fail its own checks anyway), `BudgetError` when the
/// stored budget cannot be parsed (fail CLOSED), and propagates DB errors so the
/// caller can fail open on them.
pub fn fetchBudgetAndSpend(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    fleet_id: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) !?struct { budget: FleetBudget, spend: Spend } {
    const floors = windowFloors(now_ms);
    var q = PgQuery.from(try conn.query(SELECT_BUDGET_AND_SPEND_SQL, .{ fleet_id, workspace_id, fleet_id, floors.day, floors.month }));
    defer q.deinit();
    const row = try q.next() orelse return null;

    const budget_json = try row.get(?[]const u8, 0) orelse return BudgetError.UnreadableBudget;
    const spend = Spend{
        .day_nanos = try row.get(i64, 1),
        .month_nanos = try row.get(i64, 2),
    };
    return .{ .budget = try parseStoredBudget(alloc, budget_json), .spend = spend };
}

/// Parse the stored budget subobject through the SAME validator that accepted it
/// at ingest (`config_helpers.parseFleetBudget`), so the ceiling that admits a
/// run and the ceiling that kills it can never be interpreted two ways.
fn parseStoredBudget(alloc: std.mem.Allocator, budget_json: []const u8) !FleetBudget {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, budget_json, .{}) catch
        return BudgetError.UnreadableBudget;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return BudgetError.UnreadableBudget,
    };
    return config_helpers.parseFleetBudget(obj) catch BudgetError.UnreadableBudget;
}

// ── Unit tests: the pure half. Time and spend are arguments, never reads. ────

const testing = std.testing;

test "dollarsToNanos rounds to nearest and never wraps" {
    // The expected nano value IS the assertion. Naming these after
    // `NANOS_PER_USD` would let a change to that constant silently rewrite the
    // expectation the test exists to pin.
    // pin test: literal is the contract
    try testing.expectEqual(@as(i64, 1_000_000_000), dollarsToNanos(1.0));
    try testing.expectEqual(@as(i64, 1), dollarsToNanos(0.000000001));
    // pin test: literal is the contract — the parser's max daily ceiling in nanos.
    try testing.expectEqual(@as(i64, 1_000_000_000_000), dollarsToNanos(1000.0));
    try testing.expectEqual(@as(i64, 5_500_000_000), dollarsToNanos(5.5));
    // Non-finite / non-positive ceilings collapse to a ZERO cap, which `covers`
    // then refuses — never to an unbounded one.
    try testing.expectEqual(@as(i64, 0), dollarsToNanos(0.0));
    try testing.expectEqual(@as(i64, 0), dollarsToNanos(-1.0));
    try testing.expectEqual(@as(i64, 0), dollarsToNanos(std.math.nan(f64)));
    try testing.expectEqual(@as(i64, 0), dollarsToNanos(std.math.inf(f64)));
    // A finite value large enough to overflow i64 nanos saturates rather than
    // wrapping into a negative (and therefore trivially-exceeded) ceiling.
    try testing.expectEqual(@as(i64, std.math.maxInt(i64)), dollarsToNanos(1e30));
}

test "a zero-collapsed ceiling refuses every run rather than admitting one" {
    const broken = FleetBudget{ .daily_dollars = std.math.nan(f64), .monthly_dollars = null };
    try testing.expectEqual(Verdict.day_exceeded, covers(broken, .{ .day_nanos = 0, .month_nanos = 0 }));
}

test "covers admits below the daily cap and refuses at or above it" {
    const budget = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = null };
    // One nano under, exactly at, and one nano over a $1.00 ceiling. The
    // boundary is the whole point of the test.
    try testing.expectEqual(Verdict.ok, covers(budget, .{ .day_nanos = 999_999_999, .month_nanos = 0 }));
    // pin test: literal is the contract
    try testing.expectEqual(Verdict.day_exceeded, covers(budget, .{ .day_nanos = 1_000_000_000, .month_nanos = 0 }));
    try testing.expectEqual(Verdict.day_exceeded, covers(budget, .{ .day_nanos = 1_000_000_001, .month_nanos = 0 }));
}

test "covers treats an absent monthly ceiling as unlimited" {
    const budget = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = null };
    // Astronomically over any plausible month figure ($1M of spend), but the
    // day is clear — with no monthly ceiling declared, it must still be admitted.
    // pin test: literal is the contract
    try testing.expectEqual(Verdict.ok, covers(budget, .{ .day_nanos = 0, .month_nanos = 1_000_000_000_000_000 }));
}

test "covers enforces the monthly ceiling when present" {
    const budget = FleetBudget{ .daily_dollars = 100.0, .monthly_dollars = 10.0 };
    try testing.expectEqual(Verdict.ok, covers(budget, .{ .day_nanos = 0, .month_nanos = 9_999_999_999 }));
    try testing.expectEqual(Verdict.month_exceeded, covers(budget, .{ .day_nanos = 0, .month_nanos = 10_000_000_000 }));
}

test "covers reports the daily breach first when both ceilings are exceeded" {
    // The day is the tighter, more actionable signal; the operator raises the
    // daily cap or fixes the loop before ever reaching the month.
    const budget = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = 1.0 };
    try testing.expectEqual(Verdict.day_exceeded, covers(budget, .{ .day_nanos = 2_000_000_000, .month_nanos = 2_000_000_000 }));
}

test "Verdict.refused is true for exactly the two breach cases" {
    try testing.expect(!Verdict.ok.refused());
    try testing.expect(Verdict.day_exceeded.refused());
    try testing.expect(Verdict.month_exceeded.refused());
}

test "windowFloors derives both bounds from one now_ms" {
    // 2026-07-10T16:04:00Z
    const now: i64 = 1_783_699_440_000;
    const floors = windowFloors(now);
    try testing.expectEqual(now - std.time.ms_per_day, floors.day);
    try testing.expectEqual(@as(i64, 1_782_864_000_000), floors.month); // 2026-07-01T00:00:00Z
    // The month floor is never after the day floor within the first 24h of a
    // month, and both are <= now — the scan bound `LEAST(day, month)` is sound.
    try testing.expect(floors.day <= now and floors.month <= now);
}

test "parseStoredBudget rejects malformed budgets rather than admitting them" {
    // Valid.
    const ok = try parseStoredBudget(testing.allocator, "{\"daily_dollars\": 5.0}");
    try testing.expectEqual(@as(f64, 5.0), ok.daily_dollars);
    try testing.expectEqual(@as(?f64, null), ok.monthly_dollars);

    // Negative, zero, over-bound, wrong shape, and non-JSON all fail CLOSED.
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": -1}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": 0}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": 1001}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "[]"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "not json"));
}

test "parseStoredBudget round-trips a monthly ceiling" {
    const b = try parseStoredBudget(testing.allocator, "{\"daily_dollars\": 1.0, \"monthly_dollars\": 8.0}");
    try testing.expectEqual(@as(f64, 1.0), b.daily_dollars);
    try testing.expectEqual(@as(?f64, 8.0), b.monthly_dollars);
}
