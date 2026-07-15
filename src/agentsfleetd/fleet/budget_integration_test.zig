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
const FLEET_A = "fleet-budget-a";
const FLEET_B = "fleet-budget-b";
const FIXTURE_MODEL = "budget-test-model";

// 2026-07-10T16:04:00Z — comfortably mid-month, so `now - 24h` stays inside the
// same calendar month and the two windows are independently observable.
const NOW_MS: i64 = 1_783_699_440_000;
const MONTH_START_MS: i64 = 1_782_864_000_000; // 2026-07-01T00:00:00Z
const HOUR_MS: i64 = 60 * 60 * 1000;

/// The budget query sums the per-slice ledger (`fleet.metering_periods`) by each
/// slice's own `created_at`, joined to the stage telemetry row for fleet scope.
/// A metering uid must be uuidv7-shaped (char 15 = '7', the schema CHECK), so we
/// force it on a random uuid rather than mint one in Zig.
const INSERT_METERING_SLICE_SQL =
    \\INSERT INTO fleet.metering_periods
    \\  (uid, event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
    \\   run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
    \\VALUES (overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
    \\        $1, $2, 0, 0, 0, 0, 0, 0, $3, $4)
;

/// Seed one stage drain the budget query will count: a `metering_periods` slice
/// carrying `nanos` at `drain_ms`, plus the joined stage telemetry row stamped
/// at `start_ms` (the run start the query prunes by). The telemetry row's own
/// `credit_deducted_nanos` is NOT read for stage — the slice's `charged_nanos`
/// is — so it is 0.
fn seedStageSlice(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, slice_seq: i64, nanos: i64, start_ms: i64, drain_ms: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 0,
        .recorded_at = start_ms,
    });
    _ = try conn.exec(INSERT_METERING_SLICE_SQL, .{ event_id, slice_seq, nanos, drain_ms });
}

/// The common single-slice case: a run that drains `nanos` in one shot at
/// `recorded_at` (start == drain), so the fix's start-vs-drain distinction
/// collapses and the old call sites keep their exact numbers.
fn seedSpend(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, nanos: i64, recorded_at: i64) !void {
    try seedStageSlice(conn, workspace_id, fleet_id, event_id, 1, nanos, recorded_at, recorded_at);
}

fn teardownSpend(conn: *pg.Conn, workspace_id: []const u8) void {
    // metering_periods has no workspace column — delete its rows via the
    // telemetry rows they join to, BEFORE those telemetry rows are removed.
    _ = conn.exec(
        \\DELETE FROM fleet.metering_periods mp
        \\USING core.fleet_execution_telemetry t
        \\WHERE t.event_id = mp.event_id AND t.workspace_id = $1
    , .{workspace_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// A `core.fleets` row whose `config_json` carries exactly the budget under test.
fn seedFleetWithBudget(conn: *pg.Conn, fleet_uuid: []const u8, workspace_id: []const u8, budget_json: []const u8) !void {
    const config = try std.fmt.allocPrint(ALLOC, "{{\"x-agentsfleet\":{{\"budget\":{s}}}}}", .{budget_json});
    defer ALLOC.free(config);
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'budget-fixture', '', $3::jsonb, 'active', 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET config_json = EXCLUDED.config_json
    , .{ fleet_uuid, workspace_id, config });
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
    defer teardownSpend(conn, WS_A);

    // 23h ago: inside the rolling day. 25h ago: outside it, but the same month.
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-23h", 100, NOW_MS - 23 * HOUR_MS);
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-25h", 700, NOW_MS - 25 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 100), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 800), spend.month_nanos);
}

test "integration: spend is timed by DRAIN, not by run start (the P1 regression)" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownSpend(conn, WS_A);

    // A long run (12h, the max): STARTED 25h ago (outside the rolling 24h) and
    // DRAINED its money 13h ago (inside it). The accumulating telemetry stage row
    // is pinned at the 25h-ago start — the exact bug the review found. If the
    // query bucketed by that `recorded_at`, this spend would wrongly drop out of
    // the day window and the daily ceiling could be breached ~2x. Bucketing by
    // the slice's own `created_at` counts it. This test fails on the pre-fix query.
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-longrun", 1, 500, NOW_MS - 25 * HOUR_MS, NOW_MS - 13 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 500), spend.day_nanos); // drained inside 24h → counted
    try std.testing.expectEqual(@as(i64, 500), spend.month_nanos);
}

test "integration: a multi-slice run counts each slice in the window it drained" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownSpend(conn, WS_A);

    // One event, three renewal slices under one stage row started 25h ago.
    // Slice 1 drained 25h ago (OUTSIDE the day window), slices 2 and 3 drained
    // 20h and 13h ago (INSIDE; the run's 12h span reaches to 13h ago). Day spend
    // must be slices 2+3 only; month spend all three. Proves per-slice
    // attribution, not whole-run.
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-multi", 1, 100, NOW_MS - 25 * HOUR_MS, NOW_MS - 25 * HOUR_MS);
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-multi", 2, 20, NOW_MS - 25 * HOUR_MS, NOW_MS - 20 * HOUR_MS);
    try seedStageSlice(conn, WS_A, FLEET_A, "evt-budget-multi", 3, 3, NOW_MS - 25 * HOUR_MS, NOW_MS - 13 * HOUR_MS);

    const spend = (try budget.spendForFleetOn(conn, WS_A, FLEET_A, NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 23), spend.day_nanos); // slices 2+3
    try std.testing.expectEqual(@as(i64, 123), spend.month_nanos); // all three (same month)
}

test "integration: spend_for_fleet_excludes_rows_before_the_calendar_month_start" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try uc1.seed(conn, WS_A);
    defer uc1.teardown(conn, WS_A);
    defer teardownSpend(conn, WS_A);

    // One millisecond before the month began → counted by neither window.
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-lastmonth", 5000, MONTH_START_MS - 1);
    // Exactly at the month start → counted by the month (the bound is inclusive).
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
    defer teardownSpend(conn, WS_A);
    defer teardownSpend(conn, WS_B);

    const recent = NOW_MS - HOUR_MS;
    try seedSpend(conn, WS_A, FLEET_A, "evt-budget-mine", 42, recent);
    try seedSpend(conn, WS_A, FLEET_B, "evt-budget-sibling", 999, recent); // same workspace, other fleet
    try seedSpend(conn, WS_B, FLEET_A, "evt-budget-foreign", 999, recent); // same fleet name, other workspace

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
    const spend = (try budget.spendForFleetOn(conn, WS_A, "fleet-never-ran", NOW_MS)).?;
    try std.testing.expectEqual(@as(i64, 0), spend.day_nanos);
    try std.testing.expectEqual(@as(i64, 0), spend.month_nanos);
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
