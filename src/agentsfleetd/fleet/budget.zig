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

/// A fleet's stored ceiling paired with what it has already spent.
pub const BudgetAndSpend = struct { budget: FleetBudget, spend: Spend };

/// What the renew-side read observed. Splitting "we read nothing" into three
/// distinct causes is what lets the two failure postures be *decided* by a pure
/// function (`refusalFor`) rather than buried in a `catch` beside a connection.
pub const BudgetRead = union(enum) {
    found: BudgetAndSpend,
    /// Nothing to enforce: no fleet row (the lease's own checks own that case),
    /// or a fleet that declares no `budget` at all. Undeclared is unbounded —
    /// killing such a run would enforce a ceiling nobody wrote.
    absent,
    /// A budget IS declared but cannot be parsed — fail CLOSED.
    unreadable,
    /// The database could not be reached or queried — fail OPEN.
    unavailable,
};

/// The mid-run refusal decision. Returns the breach verdict when the run must
/// stop, `null` when it may continue.
///
/// The whole asymmetry lives here, in one switch, testable without a database:
/// an *unavailable* budget admits (a metering outage must not kill every
/// in-flight run), an *unreadable* one refuses (a ceiling we cannot parse is not
/// a ceiling we may ignore).
pub fn refusalFor(read: BudgetRead) ?Verdict {
    return switch (read) {
        .found => |f| {
            const verdict = covers(f.budget, f.spend);
            return if (verdict.refused()) verdict else null;
        },
        .absent, .unavailable => null,
        .unreadable => .day_exceeded,
    };
}

/// The pre-run admission decision. A spend we could not read admits the event
/// (fail open), mirroring `metering.balanceCoversEstimate`.
pub fn verdictOrAdmit(maybe_spend: ?Spend, fleet_budget: FleetBudget) Verdict {
    const spend = maybe_spend orelse return .ok;
    return covers(fleet_budget, spend);
}

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
    return spendForFleetOn(conn, workspace_id, fleet_id, now_ms) catch |err| {
        log.warn("budget_spend_query_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .err = @errorName(err) });
        return null;
    };
}

/// The connection-taking half of `spendForFleet`, so a caller that already holds
/// a connection (the renew gate, the integration tests) does not take a second
/// one from the pool. A fleet with no telemetry rows yet spends zero.
pub fn spendForFleetOn(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, now_ms: i64) !?Spend {
    const floors = windowFloors(now_ms);
    var q = PgQuery.from(try conn.query(SELECT_SPEND_SQL, .{ workspace_id, fleet_id, floors.day, floors.month }));
    defer q.deinit();
    const row = try q.next() orelse return Spend{ .day_nanos = 0, .month_nanos = 0 };
    return Spend{
        .day_nanos = try row.get(i64, 0),
        .month_nanos = try row.get(i64, 1),
    };
}

/// Classify the renew-side read into the three no-budget causes plus the happy
/// path, so `refusalFor` can decide without touching a connection.
pub fn readBudget(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    fleet_id: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) BudgetRead {
    const found = fetchBudgetAndSpend(conn, alloc, fleet_id, workspace_id, now_ms) catch |err| {
        if (err == BudgetError.UnreadableBudget) return .unreadable;
        log.warn("budget_read_query_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .err = @errorName(err) });
        return .unavailable;
    } orelse return .absent;
    return .{ .found = found };
}

/// Renew-side read. Returns `null` when there is no ceiling to enforce — either
/// the fleet row is gone (the lease's own checks own that case) or the fleet
/// declares no `budget` at all. Returns `BudgetError.UnreadableBudget` when a
/// budget IS declared but cannot be parsed (fail CLOSED), and propagates DB
/// errors so the caller can fail open on them.
///
/// The absent/malformed split is load-bearing. `parseFleetConfig` requires
/// `budget`, so a live fleet always has one — but `config_json` rows written by
/// other paths (fixtures, anything predating the requirement) may not, and
/// refusing THEIR renewals would kill healthy in-flight runs to enforce a
/// ceiling nobody declared. Undeclared means unbounded here, exactly as it was
/// before this gate existed; the tenant credit pool still bounds it.
pub fn fetchBudgetAndSpend(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    fleet_id: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) !?BudgetAndSpend {
    const floors = windowFloors(now_ms);
    var q = PgQuery.from(try conn.query(SELECT_BUDGET_AND_SPEND_SQL, .{ fleet_id, workspace_id, fleet_id, floors.day, floors.month }));
    defer q.deinit();
    const row = try q.next() orelse return null; // no fleet row
    // SQL NULL here means the JSON path found no `budget` key: no ceiling was
    // declared, so there is nothing to enforce. A DECLARED-but-malformed budget
    // is a different thing and fails closed inside `parseStoredBudget`.
    const budget_json = try row.get(?[]const u8, 0) orelse return null;
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

// ── Inline tests for the module-private helpers ─────────────────────────────
// The public surface is tested in `budget_test.zig`; these two reach functions
// that stay private, so they live beside them.

const testing = std.testing;

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
