//! Integration tests for budget.zig — the halves that only a real Postgres can
//! prove: the windowed `SUM(credit_deducted_nanos)` and the renew-side read of a
//! fleet's stored budget out of `core.fleets.config_json`.
//!
//! The pure ceiling math (`dollarsToNanos`, `covers`, `parseStoredBudget`) is
//! unit-tested inline in `budget.zig`; nothing here re-proves it.
//!
//! Time is always an argument (`NOW_MS`), never `clock.nowMillis()` — the window
//! boundaries are the whole point, and a wall-clock read would make them
//! untestable near a month edge (RULE TIM).

const std = @import("std");
const pg = @import("pg");

const budget = @import("budget.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const store = @import("../state/fleet_telemetry_store.zig");

const ALLOC = std.testing.allocator;

// Segment 5 (aa22) identifies this workstream's fixtures; easy to grep and clean.
const WS_A = "0195b4ba-8d3a-7f13-8abc-aa2200000001";
const WS_B = "0195b4ba-8d3a-7f13-8abc-aa2200000002";

// Fleet identifiers are UUIDs behind a real foreign key, so a drain fixture has
// to create the fleet it charges: `billing.usage_ledger.fleet_id` will not take
// an invented name the way the bare TEXT column it replaces did. A fleet also
// belongs to exactly one workspace, so the cross-workspace scope check uses a
// distinct fleet over there rather than the same identifier in two places.
const FLEET_A = "0195b4ba-8d3a-7f13-8abc-aa2200000011";
const FLEET_B = "0195b4ba-8d3a-7f13-8abc-aa2200000012";
const FLEET_FOREIGN = "0195b4ba-8d3a-7f13-8abc-aa2200000013";
const FLEET_NEVER_RAN = "0195b4ba-8d3a-7f13-8abc-aa2200000014";
const FIXTURE_MODEL = "budget-test-model";
const STATUS_ACTIVE = "active";
const NO_BUDGET_CONFIG = "{}";

// 2026-07-10T16:04:00Z — comfortably mid-month, so `now - 24h` stays inside the
// same calendar month and the two windows are independently observable.
const NOW_MS: i64 = 1_783_699_440_000;
const MONTH_START_MS: i64 = 1_782_864_000_000; // 2026-07-01T00:00:00Z
const HOUR_MS: i64 = 60 * 60 * 1000;

/// The rolling day's lower bound at `NOW_MS`. Every apportionment case below is
/// positioned relative to it, so the boundary arithmetic is written once.
const DAY_FLOOR_MS: i64 = NOW_MS - 24 * HOUR_MS;

/// Seed one ledger row for a run that charged `nanos` between `start_ms` and
/// `drain_ms`. One row now carries a whole run, so a fixture states the run's
/// span rather than a single instant, and the drain attributes the covered
/// fraction of the total to each window.
fn seedStageSlice(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, nanos: i64, start_ms: i64, drain_ms: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = nanos,
        .event_created_at = start_ms,
        .created_at = start_ms,
        // The run charged from `start_ms` to `drain_ms`; the drain apportions
        // across that span instead of stamping the whole total on one instant.
        .last_charged_at = drain_ms,
    });
}

/// The common single-slice case: a run that drains `nanos` in one shot at
/// `recorded_at` (start == drain), so the fix's start-vs-drain distinction
/// collapses and the old call sites keep their exact numbers.
fn seedSpend(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, nanos: i64, recorded_at: i64) !void {
    try seedStageSlice(conn, workspace_id, fleet_id, event_id, nanos, recorded_at, recorded_at);
}

/// Runs BEFORE the workspace teardown that would `SET NULL` the scope columns
/// this delete filters on (`defer` is last-in-first-out, so registering it after
/// `uc1.teardown` is what puts it first).
fn teardownSpend(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM billing.usage_ledger WHERE workspace_id = $1::uuid", .{workspace_id}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// A `core.fleets` row a fixture can charge against. `name` is unique per
/// workspace (`uq_fleets_workspace_id_name`), and the tenant must be the one the
/// workspace already belongs to — the fleet's `(workspace_id, tenant_id)` pair is
/// constrained against `core.workspaces`, so an arbitrary tenant is rejected.
fn seedFleet(conn: *pg.Conn, fleet_uuid: []const u8, workspace_id: []const u8, name: []const u8, config_json: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, '', $5::jsonb, $6, 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET config_json = EXCLUDED.config_json
    , .{ fleet_uuid, workspace_id, base.TEST_TENANT_ID, name, config_json, STATUS_ACTIVE });
}

/// A `core.fleets` row whose `config_json` carries exactly the budget under test.
fn seedFleetWithBudget(conn: *pg.Conn, fleet_uuid: []const u8, workspace_id: []const u8, budget_json: []const u8) !void {
    const config = try std.fmt.allocPrint(ALLOC, "{{\"x-agentsfleet\":{{\"budget\":{s}}}}}", .{budget_json});
    defer ALLOC.free(config);
    try seedFleet(conn, fleet_uuid, workspace_id, "budget-fixture", config);
}

fn teardownFleet(conn: *pg.Conn, fleet_uuid: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_uuid}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
}

// ── spendForFleetOn: the two windows, and who they count ────────────────────

test "integration: spend_for_fleet_counts_only_the_rolling_day_inside_the_day_window" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // 23h ago: inside the rolling day. 25h ago: outside it, but the same month.
    // Both are one-shot charges, so each span is a point and apportionment
    // degenerates to the all-or-nothing this case always tested.
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-23h", 100, NOW_MS - 23 * HOUR_MS);
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-25h", 700, NOW_MS - 25 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 100), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 800), spend.month_nanos);
}

test "integration: a long run's spend does not fall out of the day window" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // A 12h run (the maximum) that STARTED 25h ago and stopped charging 13h ago.
    // Its one accumulating row is stamped `created_at` at the 25h-ago start, so a
    // query bucketing on that single instant drops the whole spend out of the
    // rolling day and lets the daily ceiling be breached about twice over. The
    // guard is that this is not zero: eleven of the run's twelve hours sit inside
    // the window, so 500 * 11/12 = 458.33, rounded by the `::bigint` cast.
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-longrun", 500, NOW_MS - 25 * HOUR_MS, NOW_MS - 13 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 458), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 500), spend.month_nanos); // wholly inside the month
}

test "integration: an accumulated run is apportioned, not counted at one instant" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // One event's renewals accumulate into ONE row — the ledger holds a single
    // row per (event_id, charge_type) — so three slices of 100, 20 and 3 charged
    // between 25h and 13h ago are stored as 123 spanning those twelve hours.
    //
    // This is the case where apportionment and per-slice attribution disagree.
    // The slices were front-loaded, so counting the two inside the window gives
    // 23, while spreading 123 evenly over the span gives 123 * 11/12 = 112.75.
    // Apportionment assumes even spread; real runs are near-uniform (a
    // time-based run fee plus ~20s renewals), so this fixture is the adversarial
    // shape rather than the typical one, and it is pinned deliberately.
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-multi", 123, NOW_MS - 25 * HOUR_MS, NOW_MS - 13 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 113), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 123), spend.month_nanos);
}

test "integration: spend_for_fleet_excludes_rows_before_the_calendar_month_start" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // One millisecond before the month began → counted by neither window.
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-lastmonth", 5000, MONTH_START_MS - 1);
    // Exactly at the month start → counted by the month (the bound is inclusive).
    // This is the point-span-on-the-floor case: the apportionment's first arm
    // has to test `< floor` rather than `<= floor`, or a charge stamped exactly
    // on the boundary is discarded while the row filter's `>=` admits it.
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-monthstart", 11, MONTH_START_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 0), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 11), spend.month_nanos);
}

test "integration: spend_for_fleet_is_scoped_to_one_fleet_and_one_workspace" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    try uc1.seed(conn, WS_B);
    defer uc1.teardown(conn, WS_A);
    defer uc1.teardown(conn, WS_B);
    defer teardownFleet(conn, FLEET_A);
    defer teardownFleet(conn, FLEET_B);
    defer teardownFleet(conn, FLEET_FOREIGN);
    defer teardownSpend(conn, WS_A);
    defer teardownSpend(conn, WS_B);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);
    try seedFleet(conn, FLEET_B, WS_A, "budget-b", NO_BUDGET_CONFIG);
    try seedFleet(conn, FLEET_FOREIGN, WS_B, "budget-foreign", NO_BUDGET_CONFIG);

    const recent = NOW_MS - HOUR_MS;
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-mine", 42, recent);
    try seedSpend(conn, WS_A, FLEET_B, "evt-budget-sibling", 999, recent); // same workspace, other fleet
    try seedSpend(conn, WS_B, FLEET_FOREIGN, "evt-budget-foreign", 999, recent); // other workspace

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 42), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 42), spend.month_nanos);
}

test "integration: spend_for_fleet_reports_zero_for_a_fleet_that_has_never_run" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);

    // Zero spend must be a real Spend, never null — null means "could not tell",
    // and the gates fail OPEN on null. A brand-new fleet must be admitted on its
    // merits, not on an unreadable-spend fallback.
    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_NEVER_RAN, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 0), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 0), spend.month_nanos);
}

// ── Apportionment: where a run's span meets the window floor ────────────────
//
// One ledger row carries a whole run, and a run can outlast the rolling window
// it is checked against, so the drain splits the accumulated total by how much
// of `[created_at, last_charged_at]` the window covers. Every case below is a
// position of that span relative to the day floor; none of it is provable
// without a database, because it is entirely SQL arithmetic.

/// Both windows for one span, so a case states its two expectations together.
fn drainFor(conn: *pg.Conn, event_id: []const u8, nanos: i64, start_ms: i64, drain_ms: i64) !budget.Spend {
    try seedStageSlice(conn, WS_A, FLEET_A, event_id, nanos, start_ms, drain_ms);
    return (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
}

test "integration: a span straddling the day floor contributes its covered fraction" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // Twelve hours of run, one of them before the floor: eleven twelfths land
    // inside. 1200 is chosen so the share divides exactly and the assertion is
    // not also testing the rounding of the `::bigint` cast.
    const spend = try drainFor(conn, "evt-apportion-straddle", 1200, DAY_FLOOR_MS - HOUR_MS, DAY_FLOOR_MS + 11 * HOUR_MS);
    try std.testing.expectEqual(@as(i64, 1100), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 1200), spend.month_nanos); // wholly inside the month
}

test "integration: a span entirely before the day floor contributes nothing" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    const spend = try drainFor(conn, "evt-apportion-before", 900, DAY_FLOOR_MS - 5 * HOUR_MS, DAY_FLOOR_MS - HOUR_MS);
    try std.testing.expectEqual(@as(i64, 0), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 900), spend.month_nanos);
}

test "integration: a span entirely after the day floor contributes all of it" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    const spend = try drainFor(conn, "evt-apportion-after", 900, DAY_FLOOR_MS + HOUR_MS, DAY_FLOOR_MS + 2 * HOUR_MS);
    try std.testing.expectEqual(@as(i64, 900), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 900), spend.month_nanos);
}

test "integration: a point span exactly on the day floor is inside the window" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // The lower bound is inclusive, matching the row filter's `>=`. A one-shot
    // charge whose span is a single instant ON the floor counts in full; the
    // fraction arm never sees it, so it cannot divide by a zero-length span.
    const spend = try drainFor(conn, "evt-apportion-onfloor", 900, DAY_FLOOR_MS, DAY_FLOOR_MS);
    try std.testing.expectEqual(@as(i64, 900), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 900), spend.month_nanos);
}

test "integration: a run that stopped charging exactly on the floor contributes nothing" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // The complement of the case above, and why the inclusive bound costs
    // nothing: a real span ending ON the floor reaches the fraction arm with a
    // zero numerator, so it contributes zero without needing the first arm to
    // catch it.
    const spend = try drainFor(conn, "evt-apportion-endsonfloor", 900, DAY_FLOOR_MS - 2 * HOUR_MS, DAY_FLOOR_MS);
    try std.testing.expectEqual(@as(i64, 0), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 900), spend.month_nanos);
}

test "integration: a growing span moves more of the run inside the window" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_A);
    defer teardownSpend(conn, WS_A);
    try seedFleet(conn, FLEET_A, WS_A, "budget-a", NO_BUDGET_CONFIG);

    // A live run renews repeatedly: the total accumulates and `last_charged_at`
    // advances, so the covered fraction is recomputed on every gate check rather
    // than fixed when the row was born. Started 2h before the floor; after the
    // first renewal 2h of a 4h span are inside (half), after the second 6h of an
    // 8h span (three quarters).
    const started = DAY_FLOOR_MS - 2 * HOUR_MS;
    const first = try drainFor(conn, "evt-apportion-growing", 400, started, started + 4 * HOUR_MS);
    try std.testing.expectEqual(@as(i64, 200), first.day_nanos);

    // The renewal path accumulates in place and pushes the cursor forward; the
    // fixture does the same by hand, because the fenced statement that owns this
    // write cannot be driven from here without a lease.
    _ = try conn.exec(
        \\UPDATE billing.usage_ledger
        \\SET credit_deducted_nanos = $2, last_charged_at = $3
        \\WHERE event_id = $1
    , .{ "evt-apportion-growing", @as(i64, 800), started + 8 * HOUR_MS });

    const second = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 600), second.day_nanos);
    try std.testing.expectEqual(@as(i64, 800), second.month_nanos);
}

// ── fetchBudgetAndSpend: the renew-side read ────────────────────────────────

const FLEET_UUID = "0195b4ba-8d3a-7f13-8abc-aa2200000101";
/// Never inserted by any test in this file. The "fleet row is gone" case must not
/// depend on a sibling test's teardown having succeeded — a swallowed teardown
/// error would otherwise turn a leaked row into a spurious failure here.
const FLEET_UUID_ABSENT = "0195b4ba-8d3a-7f13-8abc-aa22000001ff";

test "integration: fetch_budget_and_spend_reads_the_stored_ceiling_and_both_windows" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_UUID);
    defer teardownSpend(conn, WS_A);

    try seedFleetWithBudget(conn, FLEET_UUID, WS_A, "{\"daily_dollars\": 5.0, \"monthly_dollars\": 8.0}");
    try seedSpend(conn, WS_A, FLEET_UUID, "evt-budget-fetch-1", 300, NOW_MS - HOUR_MS);
    try seedSpend(conn, WS_A, FLEET_UUID, "evt-budget-fetch-2", 40, NOW_MS - 30 * HOUR_MS); // month only

    const found = (try budget.fetchBudgetAndSpend(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(f64, 5.0), found.budget.daily_dollars);
    try std.testing.expectEqual(@as(?f64, 8.0), found.budget.monthly_dollars);
    try std.testing.expectEqual(@as(i64, 300), found.spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 340), found.spend.month_nanos);

    // Well under a $5 ceiling — the run continues.
    try std.testing.expectEqual(budget.Verdict.ok, budget.covers(found.budget, found.spend));
}

test "integration: fetch_budget_and_spend_refuses_an_unparseable_stored_budget" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_UUID);

    // A hand-edited, invalid ceiling. Fails CLOSED: a budget we cannot read is
    // not a budget we may ignore. (Distinct from a DB fault, which fails open.)
    try seedFleetWithBudget(conn, FLEET_UUID, WS_A, "{\"daily_dollars\": -1}");
    try std.testing.expectError(
        budget.BudgetError.UnreadableBudget,
        budget.fetchBudgetAndSpend(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS),
    );
}

/// Poison the connection's transaction so the NEXT query on it errors, without
/// touching any shared table. A divide-by-zero aborts the transaction; every
/// subsequent statement returns `error.PG` ("current transaction is aborted")
/// until rollback. Deterministic, isolated, and reversible — a clean DB-fault
/// injection for the fail-open paths.
fn poisonTransaction(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
    // The abort is the whole point, so the error is expected, not suppressed.
    try std.testing.expectError(error.PG, conn.exec("SELECT 1/0", .{}));
}

fn healTransaction(conn: *pg.Conn) void {
    _ = conn.exec("ROLLBACK", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "integration: the spend query surfaces a DB fault as an error the pool gate catches" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // The pre-run fail-open is two halves: `spendForFleet` (pool) turns a query
    // error into null via `catch`, and `verdictOrAdmit(null,…)` admits (unit
    // test). This proves the FIRST half is reachable — the query genuinely errors
    // on a DB fault, so the `catch` is not dead. (The pool-level catch itself
    // can't be poisoned here: `spendForFleet` acquires a fresh connection.) The
    // exact error variant (PG "txn aborted" vs ConnectionBusy) is driver-drain
    // dependent and immaterial — the `catch |err|` swallows any of them.
    try poisonTransaction(conn);
    defer healTransaction(conn);
    if (budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)) |_| {
        return error.TestExpectedQueryToFailOnPoisonedTxn;
    } else |_| {}
}

test "integration: readBudget classifies a query error as unavailable (fail open)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // The renew-side twin: a DB fault must map to `.unavailable`, which
    // `refusalFor` admits — NOT to `.unreadable`, which would refuse. Conflating
    // "could not ask" with "answer was nonsense" would kill in-flight runs during
    // any metering blip.
    try poisonTransaction(conn);
    defer healTransaction(conn);
    const read = budget.readBudget(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS);
    try std.testing.expectEqual(std.meta.Tag(budget.BudgetRead).unavailable, std.meta.activeTag(read));
    try std.testing.expectEqual(@as(?budget.Verdict, null), budget.refusalFor(read));
}

test "integration: fetch_budget_and_spend_admits_a_fleet_that_declares_no_budget" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_UUID);

    // `config_json` with no budget subobject: the JSON path yields SQL NULL.
    // "No ceiling declared" is NOT "ceiling we cannot read". Refusing here would
    // kill the in-flight runs of every fleet row written by a path that does not
    // set `budget` — enforcing a limit nobody wrote. `service_token_splits_wire_test`
    // seeds exactly such a fleet (`config_json = "{}"`), and caught this.
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'budget-fixture', '', '{"x-agentsfleet":{}}'::jsonb, 'active', 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET config_json = EXCLUDED.config_json
    , .{ FLEET_UUID, WS_A });

    const found = try budget.fetchBudgetAndSpend(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS);
    try std.testing.expectEqual(@as(@TypeOf(found), null), found);
    // ...and the read classifies as `.absent`, which `refusalFor` admits.
    const read = budget.readBudget(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS);
    try std.testing.expectEqual(std.meta.Tag(budget.BudgetRead).absent, std.meta.activeTag(read));
    try std.testing.expectEqual(@as(?budget.Verdict, null), budget.refusalFor(.absent));
}

test "integration: a budget key holding JSON null admits (not a declared ceiling)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_UUID);

    // `budget: null` is NOT SQL NULL — `config_json->'x-agentsfleet'->'budget'`
    // yields JSONB null, which `::text` renders as the string "null". Before the
    // fix this flowed to `parseStoredBudget` → `.unreadable` → refused, killing
    // in-flight runs of a fleet that declared no ceiling. It must admit, exactly
    // like a missing key.
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'budget-fixture', '', '{"x-agentsfleet":{"budget":null}}'::jsonb, 'active', 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET config_json = EXCLUDED.config_json
    , .{ FLEET_UUID, WS_A });

    const read = budget.readBudget(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS);
    try std.testing.expectEqual(std.meta.Tag(budget.BudgetRead).absent, std.meta.activeTag(read));
    try std.testing.expectEqual(@as(?budget.Verdict, null), budget.refusalFor(read));
}

test "integration: a declared-but-malformed budget still refuses (fail closed)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownFleet(conn, FLEET_UUID);

    // The distinction: a `budget` OBJECT is present and its value is nonsense.
    // That is a ceiling someone tried to set and botched, so the run stops.
    try seedFleetWithBudget(conn, FLEET_UUID, WS_A, "{\"daily_dollars\": \"five\"}");
    const read = budget.readBudget(conn, ALLOC, FLEET_UUID, WS_A, NOW_MS);
    try std.testing.expectEqual(std.meta.Tag(budget.BudgetRead).unreadable, std.meta.activeTag(read));
    const refusal = budget.refusalFor(.unreadable);
    try std.testing.expect(refusal != null and refusal.?.refused());
}

test "integration: fetch_budget_and_spend_returns_null_when_the_fleet_row_is_gone" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // No fleet row: the lease's own checks own that case, so the budget gate
    // admits rather than inventing a refusal.
    const missing = try budget.fetchBudgetAndSpend(conn, ALLOC, FLEET_UUID_ABSENT, WS_A, NOW_MS);
    try std.testing.expectEqual(@as(@TypeOf(missing), null), missing);
}
