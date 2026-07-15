//! Unit tests for budget.zig — the pure half: the ceiling math, the two failure
//! postures, and the stored-budget parser.
//!
//! Nothing here touches a database. Time and spend arrive as arguments, so the
//! window boundaries and the fail-open/fail-closed decisions are pinned by value
//! rather than by whatever the clock and the pool happened to be doing.
//!
//! The query halves (`spendForFleetOn`, `fetchBudgetAndSpend`) are proven in
//! `budget_integration_test.zig` against real Postgres.

const std = @import("std");

const budget = @import("budget.zig");
const config_types = @import("../fleet_runtime/config_types.zig");

const FleetBudget = config_types.FleetBudget;
const Spend = budget.Spend;
const Verdict = budget.Verdict;
const BudgetError = budget.BudgetError;
const dollarsToNanos = budget.dollarsToNanos;
const covers = budget.covers;
const verdictOrAdmit = budget.verdictOrAdmit;
const refusalFor = budget.refusalFor;
const windowFloors = budget.windowFloors;
const parseStoredBudget = budget.parseStoredBudget;

/// A $1.00/day ceiling with no monthly bound, plus a spend either side of it.
const TIGHT = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = null };
const OVER_SPEND = Spend{ .day_nanos = 5_000_000_000, .month_nanos = 5_000_000_000 };
const UNDER_SPEND = Spend{ .day_nanos = 1, .month_nanos = 1 };

const testing = std.testing;

// ── The ceiling math ────────────────────────────────────────────────────────

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
    const b = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = null };
    // One nano under, exactly at, and one nano over a $1.00 ceiling. The
    // boundary is the whole point of the test.
    try testing.expectEqual(Verdict.ok, covers(b, .{ .day_nanos = 999_999_999, .month_nanos = 0 }));
    // pin test: literal is the contract
    try testing.expectEqual(Verdict.day_exceeded, covers(b, .{ .day_nanos = 1_000_000_000, .month_nanos = 0 }));
    try testing.expectEqual(Verdict.day_exceeded, covers(b, .{ .day_nanos = 1_000_000_001, .month_nanos = 0 }));
}

test "covers treats an absent monthly ceiling as unlimited" {
    const b = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = null };
    // Astronomically over any plausible month figure ($1M of spend), but the
    // day is clear — with no monthly ceiling declared, it must still be admitted.
    // pin test: literal is the contract
    try testing.expectEqual(Verdict.ok, covers(b, .{ .day_nanos = 0, .month_nanos = 1_000_000_000_000_000 }));
}

test "covers enforces the monthly ceiling when present" {
    const b = FleetBudget{ .daily_dollars = 100.0, .monthly_dollars = 10.0 };
    try testing.expectEqual(Verdict.ok, covers(b, .{ .day_nanos = 0, .month_nanos = 9_999_999_999 }));
    try testing.expectEqual(Verdict.month_exceeded, covers(b, .{ .day_nanos = 0, .month_nanos = 10_000_000_000 }));
}

test "covers reports the daily breach first when both ceilings are exceeded" {
    // The day is the tighter, more actionable signal; the operator raises the
    // daily cap or fixes the loop before ever reaching the month.
    const b = FleetBudget{ .daily_dollars = 1.0, .monthly_dollars = 1.0 };
    try testing.expectEqual(Verdict.day_exceeded, covers(b, .{ .day_nanos = 2_000_000_000, .month_nanos = 2_000_000_000 }));
}

test "Verdict.refused is true for exactly the two breach cases" {
    try testing.expect(!Verdict.ok.refused());
    try testing.expect(Verdict.day_exceeded.refused());
    try testing.expect(Verdict.month_exceeded.refused());
}

test "verdictOrAdmit fails OPEN when the spend could not be read" {
    // A metering outage must not halt every fleet on the platform. Even against
    // a ceiling this run has visibly blown, an unreadable spend admits.
    try testing.expectEqual(Verdict.ok, verdictOrAdmit(null, TIGHT));
    // ...but a spend we CAN read is still enforced.
    try testing.expectEqual(Verdict.day_exceeded, verdictOrAdmit(OVER_SPEND, TIGHT));
    try testing.expectEqual(Verdict.ok, verdictOrAdmit(UNDER_SPEND, TIGHT));
}

test "refusalFor fails OPEN on an unavailable database and on an absent fleet" {
    try testing.expectEqual(@as(?Verdict, null), refusalFor(.unavailable));
    try testing.expectEqual(@as(?Verdict, null), refusalFor(.absent));
}

test "refusalFor fails CLOSED on a stored budget it cannot parse" {
    // Asymmetric on purpose: a ceiling we cannot read is not one we may ignore.
    // Contrast with `.unavailable` above — that is "we could not ask", this is
    // "we asked and the answer was nonsense".
    const refusal = refusalFor(.unreadable);
    try testing.expect(refusal != null);
    try testing.expect(refusal.?.refused());
}

test "refusalFor refuses an over-budget read and admits an under-budget one" {
    try testing.expectEqual(
        @as(?Verdict, .day_exceeded),
        refusalFor(.{ .found = .{ .budget = TIGHT, .spend = OVER_SPEND } }),
    );
    try testing.expectEqual(
        @as(?Verdict, null),
        refusalFor(.{ .found = .{ .budget = TIGHT, .spend = UNDER_SPEND } }),
    );
}

test "refusalFor surfaces a monthly breach distinctly from a daily one" {
    const monthly = FleetBudget{ .daily_dollars = 100.0, .monthly_dollars = 1.0 };
    const spend = Spend{ .day_nanos = 0, .month_nanos = 5_000_000_000 };
    try testing.expectEqual(
        @as(?Verdict, .month_exceeded),
        refusalFor(.{ .found = .{ .budget = monthly, .spend = spend } }),
    );
}

// ── windowFloors + parseStoredBudget (pub helpers, no DB) ────────────────────

test "windowFloors derives both bounds from one now_ms" {
    // 2026-07-10T16:04:00Z
    const now: i64 = 1_783_699_440_000;
    const floors = windowFloors(now);
    try testing.expectEqual(now - std.time.ms_per_day, floors.day);
    try testing.expectEqual(@as(i64, 1_782_864_000_000), floors.month); // 2026-07-01T00:00:00Z
    // Within the first 24h of a month the month floor is >= the day floor; later
    // it is < the day floor. `LEAST(day, month)` picks the earlier as the scan
    // bound either way, and both are <= now.
    try testing.expect(floors.day <= now and floors.month <= now);
}

test "parseStoredBudget: valid object parses" {
    const ok = (try parseStoredBudget(testing.allocator, "{\"daily_dollars\": 5.0}")).?;
    try testing.expectEqual(@as(f64, 5.0), ok.daily_dollars);
    try testing.expectEqual(@as(?f64, null), ok.monthly_dollars);
}

test "parseStoredBudget: a botched budget OBJECT fails CLOSED" {
    // Someone tried to declare a ceiling and got the object wrong → refuse.
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": -1}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": 0}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{\"daily_dollars\": 1001}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "{}"));
    try testing.expectError(BudgetError.UnreadableBudget, parseStoredBudget(testing.allocator, "not json"));
}

test "parseStoredBudget: a NON-object value is not a declared ceiling -> admit" {
    // JSON null (a present-but-null `budget` key renders as the text "null"),
    // and any scalar/array in the budget slot, mean "no ceiling declared" — the
    // caller admits, exactly as for a missing key.
    try testing.expectEqual(@as(?FleetBudget, null), try parseStoredBudget(testing.allocator, "null"));
    try testing.expectEqual(@as(?FleetBudget, null), try parseStoredBudget(testing.allocator, "5"));
    try testing.expectEqual(@as(?FleetBudget, null), try parseStoredBudget(testing.allocator, "[]"));
    try testing.expectEqual(@as(?FleetBudget, null), try parseStoredBudget(testing.allocator, "\"nope\""));
}

test "parseStoredBudget round-trips a monthly ceiling" {
    const b = (try parseStoredBudget(testing.allocator, "{\"daily_dollars\": 1.0, \"monthly_dollars\": 8.0}")).?;
    try testing.expectEqual(@as(f64, 1.0), b.daily_dollars);
    try testing.expectEqual(@as(?f64, 8.0), b.monthly_dollars);
}
