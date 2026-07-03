// Integration tests for per-renewal incremental metering on the fenced renewal +
// claim-settle CTEs (`renewal.renew` / `renewal_settle.claimAndSettle`). Money
// invariants: SQL == sliceCharge by construction, debits+settle sum to the real
// total, re-sent renewal ≈0, negative Δ never credits, fenced-out charges nothing,
// wallet clamps at 0. The claim-settle also flips the lease active→reported under
// the same fence. Drives the CTEs directly with explicit `now_ms`; rates via
// `MeterInputs`. Requires LIVE_DB=1.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const renewal = @import("renewal.zig");
const renewal_settle = @import("renewal_settle.zig");
const affinity = @import("affinity.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const fleet_metering_store = @import("../state/fleet_metering_store.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const ONE_MILLION = 1_000_000;
const TEST_TOKEN_COUNT = 1000;
const TEST_CHARGE_NANOS = 1_000;
const TEST_BALANCE_NANOS = 100_000;

// A second tenant for the metering-periods cross-tenant read guard.
const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8b02";

const ALLOC = std.testing.allocator;
const PRIMARY_RUN_MS: i64 = 1000;

fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8c01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8e01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8f01";
const EVENT_ID = "evt-meter-1";

const NOW_MS: i64 = 1_900_000_000_000;
const ISSUE_MS: i64 = NOW_MS - 60_000; // lease issued 60s before "now"
const BIG_BALANCE: i64 = 1_000_000_000; // ample so no clamp unless a test wants it

// A realistic platform rate set (run fee + three token tiers, all per the
// production units). Reused across tests + as the SQL==Zig reference input.
const RATES = tenant_billing.SliceRates{
    .run_nanos_per_sec = tenant_billing.RUN_NANOS_PER_SEC,
    .input_nanos_per_mtok = 3_000_000,
    .cached_input_nanos_per_mtok = 300_000,
    .output_nanos_per_mtok = 15_000_000,
};

fn meterOf(cum_in: i64, cum_cached: i64, cum_out: i64) renewal.MeterInputs {
    return .{
        .cumulative_input = cum_in,
        .cumulative_cached = cum_cached,
        .cumulative_output = cum_out,
        .run_nanos_per_sec = RATES.run_nanos_per_sec,
        .input_nanos_per_mtok = RATES.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = RATES.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = RATES.output_nanos_per_mtok,
    };
}

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state,
        \\   labels, tenant_id, last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'meter-host', 'meter-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

// Affinity holds the authoritative metering cursor the CTE reads for Δ.
fn seedAffinity(conn: *pg.Conn, fencing_seq: i64, m_in: i64, m_cached: i64, m_out: i64, last_metered: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity (id, fleet_id, last_runner_id, fencing_seq,
        \\   leased_until, metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET fencing_seq = EXCLUDED.fencing_seq,
        \\   metered_input_tokens = EXCLUDED.metered_input_tokens,
        \\   metered_cached_tokens = EXCLUDED.metered_cached_tokens,
        \\   metered_output_tokens = EXCLUDED.metered_output_tokens,
        \\   last_metered_at_ms = EXCLUDED.last_metered_at_ms
    , .{ AFFINITY_ID, FLEET_ID, RUNNER_ID, fencing_seq, NOW_MS + ONE_MILLION, m_in, m_cached, m_out, last_metered });
}

fn seedLease(conn: *pg.Conn, fencing_token: i64, status: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id,
        \\   event_id, actor, event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test', 'chat',
        \\   '{"message":"hi"}', 0, 'platform', 'test-provider', 'test-model', 0, 0, 0, 0,
        \\   $7, $8, $9, $10, $10)
        \\ON CONFLICT (id) DO UPDATE SET fencing_token = EXCLUDED.fencing_token, status = EXCLUDED.status
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID, fencing_token, NOW_MS + ONE_MILLION, status, ISSUE_MS });
}

fn seedBalance(conn: *pg.Conn, balance: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'meter-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, balance });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM core.fleet_execution_telemetry WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

const Setup = struct { h: *TestHarness, conn: *pg.Conn };

// Common arrange: live harness + a clean tenant/workspace/runner. Fence holds
// (token == seq == 1). Caller seeds the affinity cursor + balance per test.
fn arrange(fencing: i64) !Setup {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    errdefer h.deinit();
    const conn = try h.acquireConn();
    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "renewal-metering-fleet", "{}", "# z");
    try seedRunner(conn);
    try seedAffinity(conn, fencing, 0, 0, 0, ISSUE_MS);
    try seedLease(conn, fencing, "active");
    return .{ .h = h, .conn = conn };
}

fn cleanup(s: Setup) void {
    teardown(s.conn);
    s.h.releaseConn(s.conn);
    s.h.deinit();
}

fn readBalance(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query("SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid", .{base.TEST_TENANT_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

// Assert the lease status in-place: the row-backed slice is only valid until the
// query deinits, so the comparison lives inside the helper.
fn expectLeaseStatus(conn: *pg.Conn, want: []const u8) !void {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    try std.testing.expectEqualStrings(want, try row.get([]const u8, 0));
}

const StageRow = struct { charged: i64, t_in: ?i64, t_out: ?i64, wall: ?i64, slices: i64 };

fn readStage(conn: *pg.Conn) !?StageRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT t.credit_deducted_nanos, t.token_count_input, t.token_count_output, t.wall_ms,
        \\       (SELECT count(*) FROM fleet.metering_periods mp WHERE mp.event_id = t.event_id)::bigint
        \\FROM core.fleet_execution_telemetry t
        \\WHERE t.event_id = $1 AND t.charge_type = 'stage'
    , .{EVENT_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return StageRow{
        .charged = try row.get(i64, 0),
        .t_in = try row.get(?i64, 1),
        .t_out = try row.get(?i64, 2),
        .wall = try row.get(?i64, 3),
        .slices = try row.get(i64, 4),
    };
}

test "renew charges run fee + token delta == sliceCharge (SQL==Zig pin)" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);

    // Δt = NOW - ISSUE = 60_000ms; deltas off a zero cursor = the cumulatives.
    const meter = meterOf(1000, 500, 800);
    const expected = tenant_billing.sliceCharge(RATES, NOW_MS - ISSUE_MS, 1000, 500, 800);

    const outcome = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meter);
    try std.testing.expect(outcome == .renewed);

    // Wallet drained by exactly the Zig reference, and the breakdown row + the
    // accumulated stage row both record the same nanos — SQL == sliceCharge.
    try std.testing.expectEqual(BIG_BALANCE - expected, try readBalance(s.conn));
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(expected, stage.charged);
    try std.testing.expectEqual(@as(i64, 1), stage.slices);
    try std.testing.expectEqual(@as(?i64, TEST_TOKEN_COUNT), stage.t_in);
    try std.testing.expectEqual(@as(?i64, 800), stage.t_out);
}

test "renews + settle sum to the real total (ms-precision, non-second-aligned)" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);

    // Three non-second-aligned boundaries; cumulatives grow monotonically.
    const t1 = ISSUE_MS + 20_500;
    const t2 = ISSUE_MS + 41_250;
    const t3 = ISSUE_MS + 60_000; // settle at report
    try std.testing.expect(try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, t1, meterOf(300, 100, 200)) == .renewed);
    try std.testing.expect(try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, t2, meterOf(700, 250, 500)) == .renewed);
    _ = try renewal_settle.claimAndSettle(s.conn, LEASE_ID, RUNNER_ID, t3, meterOf(TEST_TOKEN_COUNT, 500, 800));

    // Per-slice debits telescope to one slice over the full run (elapsed t3-ISSUE, final cumulatives).
    const total = tenant_billing.sliceCharge(RATES, t3 - ISSUE_MS, 1000, 500, 800);
    try std.testing.expectEqual(BIG_BALANCE - total, try readBalance(s.conn));
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(total, stage.charged);
    try std.testing.expectEqual(@as(i64, 3), stage.slices); // 2 renews + 1 settle
    try std.testing.expectEqual(@as(?i64, TEST_TOKEN_COUNT), stage.t_in);
    try std.testing.expectEqual(@as(?i64, 800), stage.t_out);
}

test "a re-sent renewal with the same cumulatives double-bills only the run fee gap" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);

    _ = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, ISSUE_MS + 30_000, meterOf(900, 300, 600));
    const after_first = try readBalance(s.conn);
    // Retry 5ms later, SAME cumulatives → token Δ = 0; only the 5ms run fee.
    _ = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, ISSUE_MS + 30_005, meterOf(900, 300, 600));
    const retry_charge = after_first - try readBalance(s.conn);

    try std.testing.expectEqual(tenant_billing.sliceCharge(RATES, 5, 0, 0, 0), retry_charge);
    try std.testing.expect(retry_charge < tenant_billing.RUN_NANOS_PER_SEC); // ≈0, sub-second run fee
}

test "a negative delta (clock skew + token regression) never credits the balance" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);
    // Cursor AHEAD of what the body reports: metered 5000 in, last_metered in the
    // future relative to `now` → both Δt and Δtokens clamp to 0.
    try seedAffinity(s.conn, 1, 5000, 5000, 5000, NOW_MS + 10_000);

    _ = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(100, 100, 100));

    try std.testing.expectEqual(BIG_BALANCE, try readBalance(s.conn)); // never credited
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, 0), stage.charged);
}

test "a fenced-out renewal charges nothing — no debit, no telemetry, no breakdown" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);
    // A reclaim bumped the affinity sequence past the lease's token → guard fails.
    try seedAffinity(s.conn, 9, 0, 0, 0, ISSUE_MS);

    const outcome = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(1000, 500, 800));
    try std.testing.expect(outcome == .lost);

    try std.testing.expectEqual(BIG_BALANCE, try readBalance(s.conn)); // untouched
    try std.testing.expect((try readStage(s.conn)) == null); // no stage row written
}

test "claim+settle on a never-renewed run flips reported + charges the whole slice as one row" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);
    // arrange seeded an ACTIVE lease; the claim+settle flips it active→reported
    // under the fence AND charges the full run as one final slice (the run
    // finished inside one renewal window, so it was never /renew'd).

    const out = try renewal_settle.claimAndSettle(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(400, 0, 350));
    const expected = tenant_billing.sliceCharge(RATES, NOW_MS - ISSUE_MS, 400, 0, 350);

    try std.testing.expect(out.claimed); // the active→reported flip won the fence
    try std.testing.expectEqual(expected, out.charged_nanos);
    try std.testing.expectEqual(BIG_BALANCE - expected, try readBalance(s.conn));
    try expectLeaseStatus(s.conn, "reported");
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, 1), stage.slices); // the settle row only
    try std.testing.expectEqual(expected, stage.charged);
}

test "claim+settle is fenced: a superseded holder claims nothing and charges nothing" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);
    // A reclaim bumped the affinity sequence past the lease's token → the claim
    // guard fails: no active→reported flip, no debit, no telemetry.
    try seedAffinity(s.conn, 9, 0, 0, 0, ISSUE_MS);

    const out = try renewal_settle.claimAndSettle(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(400, 0, 350));

    try std.testing.expect(!out.claimed); // fenced out — the current holder wins
    try std.testing.expectEqual(@as(i64, 0), out.charged_nanos);
    try std.testing.expectEqual(BIG_BALANCE, try readBalance(s.conn)); // untouched
    try expectLeaseStatus(s.conn, "active"); // not flipped
    try std.testing.expect((try readStage(s.conn)) == null);
}

test "claim+settle on an already-reported lease claims nothing — no double-settle on report retry" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, BIG_BALANCE);
    // A prior report already flipped the lease 'reported'. The probe's status=active
    // filter must reject the retry: re-claiming would settle a SECOND final slice
    // off the same cursor (double-charge). This pins that filter.
    try seedLease(s.conn, 1, "reported");

    const out = try renewal_settle.claimAndSettle(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(400, 0, 350));

    try std.testing.expect(!out.claimed); // status ≠ active → no re-claim
    try std.testing.expectEqual(@as(i64, 0), out.charged_nanos); // no second slice
    try std.testing.expectEqual(BIG_BALANCE, try readBalance(s.conn)); // not re-debited
    try std.testing.expect((try readStage(s.conn)) == null); // no telemetry, no breakdown
}

test "claim+settle clamps an exhausting final slice to the remaining balance" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, TEST_CHARGE_NANOS); // far below the slice the run will compute

    const out = try renewal_settle.claimAndSettle(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(1000, 500, 800));

    // The settle CTE's `LEAST(slice, balance)` clamp (moved with the FLL split):
    // the wallet floors at 0 and the ledger records the CLAMPED drain, not the
    // full slice — so the audit row equals the real debit even on exhaustion.
    try std.testing.expect(out.claimed);
    try std.testing.expectEqual(@as(i64, TEST_CHARGE_NANOS), out.charged_nanos);
    try std.testing.expectEqual(@as(i64, 0), try readBalance(s.conn));
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, TEST_CHARGE_NANOS), stage.charged);
}

test "an exhausting slice clamps the wallet to zero and stamps balance_exhausted_at" {
    const s = try arrange(1);
    defer cleanup(s);
    try seedBalance(s.conn, TEST_CHARGE_NANOS); // far below the slice the run will compute

    _ = try renewal.renew(s.conn, LEASE_ID, RUNNER_ID, NOW_MS, meterOf(TEST_TOKEN_COUNT, 500, 800));

    // Balance floors at 0; the ledger records the CLAMPED drain (1000), not the
    // full slice (P2); and the exhaust stamp is set (restores is_exhausted).
    try std.testing.expectEqual(@as(i64, 0), try readBalance(s.conn));
    const stage = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, TEST_CHARGE_NANOS), stage.charged);
    var q = PgQuery.from(try s.conn.query(
        "SELECT balance_exhausted_at FROM billing.tenant_billing WHERE tenant_id = $1::uuid",
        .{base.TEST_TENANT_ID},
    ));
    defer q.deinit();
    try std.testing.expect((try (try q.next()).?.get(?i64, 0)) != null);
}

test "a fresh lease resets the affinity metering cursor to zero / issue-time" {
    const s = try arrange(1);
    defer cleanup(s);
    // A prior (completed) run left a non-zero cursor on the surviving slot.
    try seedAffinity(s.conn, 1, 9000, 4000, 7000, ISSUE_MS - TEST_BALANCE_NANOS);

    // A fresh lease issue resets it (insertLeaseRow on .fresh, fail-closed) so the
    // first /renew meters off 0/now, not the prior run — a reused slot can't over-charge.
    try affinity.resetCursor(s.conn, FLEET_ID, NOW_MS);

    var q = PgQuery.from(try s.conn.query(
        \\SELECT metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms
        \\FROM fleet.runner_affinity WHERE fleet_id = $1::uuid
    , .{FLEET_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 1));
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 2));
    try std.testing.expectEqual(NOW_MS, try row.get(i64, 3));
}

test "metering-periods read is tenant-scoped: own tenant sees slices, a foreign tenant sees none" {
    const s = try arrange(1);
    defer cleanup(s);
    // Stage telemetry carries the tenant; the store's EXISTS guard keys off it. Seed 1 stage + 2 slices.
    _ = try s.conn.exec(
        \\INSERT INTO core.fleet_execution_telemetry
        \\  (uid, id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
        \\   model, credit_deducted_nanos, recorded_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0f4101'::uuid,
        \\        'mtr_' || $2, $1::uuid, 'ws', 'z', $2, 'stage', 'platform', 'm', 225, 0)
    , .{ base.TEST_TENANT_ID, EVENT_ID });
    _ = try s.conn.exec(
        \\INSERT INTO fleet.metering_periods
        \\  (uid, event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
        \\   run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0f4102'::uuid,
        \\        $1, 1, 10, 0, 20, $2, 100, 50, 150, 0),
        \\       ('0195b4ba-8d3a-7f13-8abc-2b3e1e0f4103'::uuid,
        \\        $1, 2, 5, 0, 10, 500, 50, 25, 75, 0)
    , .{ EVENT_ID, PRIMARY_RUN_MS });

    // Owning tenant: both slices, ordered by slice_seq.
    const mine = try fleet_metering_store.listForEvent(s.conn, ALLOC, EVENT_ID, base.TEST_TENANT_ID);
    defer ALLOC.free(mine);
    try std.testing.expectEqual(@as(usize, 2), mine.len);
    try std.testing.expectEqual(@as(i64, 1), mine[0].slice_seq);
    try std.testing.expectEqual(@as(i64, 150), mine[0].charged_nanos);
    try std.testing.expectEqual(@as(i64, 2), mine[1].slice_seq);

    // A foreign tenant requesting the same event_id sees nothing (no leak).
    const theirs = try fleet_metering_store.listForEvent(s.conn, ALLOC, EVENT_ID, OTHER_TENANT_ID);
    defer ALLOC.free(theirs);
    try std.testing.expectEqual(@as(usize, 0), theirs.len);
}
