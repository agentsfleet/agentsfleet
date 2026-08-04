// Wire-level integration proof for split-token billing: a runner-shaped client
// drives the REAL router + runner_bearer middleware. The renew POST carries the
// runner's cumulative splits as its JSON body; the report POST carries the
// final splits. The server prices each slice off the wire values (rates from
// its own registry — no test code constructs MeterInputs) and advances the
// affinity cursor to the reported cumulatives; a re-sent renewal with identical
// cumulatives meters a zero-delta slice (cumulative-diff idempotency).
//
// Free-trial note: the boundary is a tenant fact (§7). arrange() closes this
// suite's tenant's trial, so the server prices every slice at the suite's own
// registry rates and the token_cost identity plus the strict non-zero arms
// assert unconditionally at any clock position. Requires LIVE_DB; skipped when
// TEST_DATABASE_URL is unset.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");
const billing_rates = @import("../state/tenant_billing_rates.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const ChargeType = @import("../state/fleet_telemetry_store.zig").ChargeType;

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8), distinct from every sibling
// suite so cross-test teardown can never race shared rows.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9c01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9f01";
const MODEL_LIBRARY_UID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01";
const EVENT_ID = "evt-wire-splits-1";
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "d" ** 64;
// Suite-private (provider, model) pair with its own seeded core.model_library
// row, so the registry resolves REAL non-zero rates for this lease (the global
// test-provider pair carries zero token rates — the >0 arms could never pass).
const PROVIDER = "wire-split-provider";
const MODEL = "wire-split-model";
const RATE_INPUT_NANOS_PER_MTOK: i64 = 3_000_000;
const RATE_CACHED_NANOS_PER_MTOK: i64 = 300_000;
const RATE_OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000;
const MODEL_CONTEXT_CAP_TOKENS: i64 = 200_000;
const BIG_BALANCE: i64 = 1_000_000_000;
const CURSOR_AGE_MS: i64 = 20_000; // the spec's 20s-cursor lease

// The wire vector under test: the run's final cumulative splits, the mid-run
// cursor the report test starts from, and the settle diffs they imply.
// Untyped so each coerces to the u32 wire fields and the i64 row reads alike.
const CUM_IN = 1000;
const CUM_CACHED = 500;
const CUM_OUT = 800;
const CURSOR_IN = 300;
const CURSOR_CACHED = 100;
const CURSOR_OUT = 200;
const SETTLE_D_IN = CUM_IN - CURSOR_IN;
const SETTLE_D_CACHED = CUM_CACHED - CURSOR_CACHED;
const SETTLE_D_OUT = CUM_OUT - CURSOR_OUT;
const LEGACY_TOTAL = CUM_IN + CUM_CACHED + CUM_OUT;

// The real DB-backed runner lookup, parked at module scope so the value
// outlives the middleware chain (tests run sequentially in one process).
// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'wire-splits-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

// Affinity holds the authoritative metering cursor the renewal CTE diffs against.
fn seedAffinity(conn: *pg.Conn, m_in: i64, m_cached: i64, m_out: i64, last_metered: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity (fleet_id, last_runner_id, fencing_seq,
        \\   leased_until, metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 1, $3, $4, $5, $6, $7, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET fencing_seq = 1,
        \\   metered_input_tokens = EXCLUDED.metered_input_tokens,
        \\   metered_cached_tokens = EXCLUDED.metered_cached_tokens,
        \\   metered_output_tokens = EXCLUDED.metered_output_tokens,
        \\   last_metered_at = EXCLUDED.last_metered_at
    , .{ FLEET_ID, RUNNER_ID, clock.nowMillis() + 600_000, m_in, m_cached, m_out, last_metered });
}

fn seedActiveLease(conn: *pg.Conn, last_metered: i64) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        'steer:test', 'chat', 0, 'platform', $7, $8,
        \\        0, 0, 0, $9, 1, $10, 'active', $11, $11)
        \\ON CONFLICT (id) DO UPDATE SET fencing_token = 1, status = 'active'
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID, PROVIDER, MODEL, last_metered, now + 60_000, now - 60_000 });
}

fn seedBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'wire-splits-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, BIG_BALANCE });
}

// Catalogue row for this suite's private (provider, model) pair + cache reseat,
// so the server's own rate resolution prices the wire deltas non-zero once the
// free-trial window closes. The cache is process-global: page_allocator, per
// the fixture convention (populate deinits any prior cache before reseating).
fn seedModelRates(conn: *pg.Conn) !void {
    const now_ms: i64 = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO UPDATE SET
        \\   input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
        \\   output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
        \\   updated_at = EXCLUDED.updated_at
    , .{ MODEL_LIBRARY_UID, MODEL, PROVIDER, MODEL_CONTEXT_CAP_TOKENS, RATE_INPUT_NANOS_PER_MTOK, RATE_CACHED_NANOS_PER_MTOK, RATE_OUTPUT_NANOS_PER_MTOK, now_ms });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM billing.usage_ledger WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    // Drop this suite's catalogue row and clear the process-global cache so
    // later suites in the same run never see the private pair. The clear is what
    // makes the drop immediate; without it the entry survives until its
    // generation check fails.
    execIgnore(conn, "DELETE FROM core.model_library WHERE provider = $1 AND model_id = $2", .{ PROVIDER, MODEL });
    model_rate_cache.clear();
    base.teardownTenant(conn);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

const Setup = struct { h: *TestHarness, conn: *pg.Conn };

// Live harness with the real runner_bearer chain + clean seeded rows; the
// caller picks the starting affinity/lease cursor.
fn arrange(cursor_in: i64, cursor_cached: i64, cursor_out: i64) !Setup {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    errdefer h.deinit();
    const conn = try h.acquireConn();
    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "service-splits-fleet", "{}", "# z");
    try seedRunner(conn);
    try seedBalance(conn);
    // §7: an open trial prices every slice to zero; close this tenant's
    // boundary so the wire proof carries the seeded registry rates.
    try base.endFreeTrialFor(conn, base.TEST_TENANT_ID);
    try seedModelRates(conn);
    const last_metered = clock.nowMillis() - CURSOR_AGE_MS;
    try seedAffinity(conn, cursor_in, cursor_cached, cursor_out, last_metered);
    try seedActiveLease(conn, last_metered);
    return .{ .h = h, .conn = conn };
}

fn cleanup(s: Setup) void {
    teardown(s.conn);
    s.h.releaseConn(s.conn);
    s.h.deinit();
}

// The pure token term of a slice, priced via the SAME registry resolution the
// handlers use — sliceCharge with no elapsed run time isolates the token
// component. Passes the closed boundary the tenant row carries; a missing
// catalogue row is an error, never a silent zero-rate fallback that would let
// a seeding regression read as a passing zero-cost identity.
fn expectedTokenCost(conn: *pg.Conn, d_in: i64, d_cached: i64, d_out: i64) !i64 {
    const rates = (try billing_rates.resolveRenewSliceRates(conn, PROVIDER, .platform, MODEL, base.TRIAL_ENDED_AT_MS, clock.nowMillis())) orelse
        return error.ModelNotPriced;
    const token_only = billing_rates.SliceRates{
        .run_nanos_per_sec = 0,
        .input_nanos_per_mtok = rates.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = rates.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = rates.output_nanos_per_mtok,
    };
    return billing_rates.sliceCharge(token_only, 0, d_in, d_cached, d_out);
}

/// The pure run-time term, the mirror of `expectedTokenCost`. The retired
/// `fleet.metering_periods` stored `run_fee_nanos` and `token_cost_nanos` as
/// separate columns; the ledger stores one `credit_deducted_nanos`. It does
/// store the `wall_ms` the fee was priced from, though, so the split is still
/// derivable — `charged - expectedRunFee(wall)` isolates the token term exactly
/// as the retired column did.
fn expectedRunFee(conn: *pg.Conn, wall_ms: i64) i64 {
    const resolved = billing_rates.resolveRenewSliceRates(conn, PROVIDER, .platform, MODEL, base.TRIAL_ENDED_AT_MS, clock.nowMillis()) catch null;
    const run_only = billing_rates.SliceRates{
        .run_nanos_per_sec = if (resolved) |r| r.run_nanos_per_sec else tenant_billing.RUN_NANOS_PER_SEC,
        .input_nanos_per_mtok = 0,
        .cached_input_nanos_per_mtok = 0,
        .output_nanos_per_mtok = 0,
    };
    return billing_rates.sliceCharge(run_only, wall_ms, 0, 0, 0);
}

/// The accumulating stage row. Token counts are cumulative across every write
/// that folded into it, so a zero-delta re-send leaves them exactly where the
/// previous write left them — which is the idempotency claim, stated on the
/// accumulator instead of on a per-slice row that no longer exists.
const StageRow = struct { t_in: i64, t_cached: i64, t_out: i64, charged: i64, wall: i64 };

fn readStage(conn: *pg.Conn) !?StageRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(token_count_input, 0), COALESCE(token_count_cached_input, 0),
        \\       COALESCE(token_count_output, 0), credit_deducted_nanos, COALESCE(wall_ms, 0)
        \\FROM billing.usage_ledger WHERE event_id = $1 AND charge_type = $2
    , .{ EVENT_ID, ChargeType.stage.label() }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return StageRow{
        .t_in = try row.get(i64, 0),
        .t_cached = try row.get(i64, 1),
        .t_out = try row.get(i64, 2),
        .charged = try row.get(i64, 3),
        .wall = try row.get(i64, 4),
    };
}

const Cursor = struct { m_in: i64, m_cached: i64, m_out: i64 };

fn readAffinityCursor(conn: *pg.Conn) !Cursor {
    var q = PgQuery.from(try conn.query(
        \\SELECT metered_input_tokens, metered_cached_tokens, metered_output_tokens
        \\FROM fleet.runner_affinity WHERE fleet_id = $1::uuid
    , .{FLEET_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.AffinityRowMissing;
    return .{ .m_in = try row.get(i64, 0), .m_cached = try row.get(i64, 1), .m_out = try row.get(i64, 2) };
}

fn expectLeaseStatus(conn: *pg.Conn, want: []const u8) !void {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.LeaseRowMissing;
    try std.testing.expectEqualStrings(want, try row.get([]const u8, 0));
}

// POST the renew with the cumulative splits as the serialized RenewRequest —
// exactly the body shape the production client sends (struct-derived, no
// hand-spelled keys).
fn postRenew(h: *TestHarness, req_body: protocol.RenewRequest) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, LEASE_ID, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const body = try std.json.Stringify.valueAlloc(ALLOC, req_body, .{});
    defer ALLOC.free(body);
    const req = try (try h.post(path).bearer(RUNNER_TOKEN)).json(body);
    return req.send();
}

test "integration: wire renew bills the body's splits, advances the cursor, and re-sends meter zero deltas" {
    const s = try arrange(0, 0, 0);
    defer cleanup(s);

    // First renewal: the cumulative wire vector off a zero cursor.
    const resp = try postRenew(s.h, .{ .input_tokens = CUM_IN, .cached_input_tokens = CUM_CACHED, .output_tokens = CUM_OUT });
    defer resp.deinit();
    try resp.expectStatus(.ok);

    // Off a zero cursor the billed deltas ARE the wire values, so the stage
    // row's counts read back the body verbatim. Subtracting the run fee the
    // stored `wall_ms` prices leaves the token term, which must equal the
    // server's own rate resolution applied to those deltas.
    const stage1 = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, CUM_IN), stage1.t_in);
    try std.testing.expectEqual(@as(i64, CUM_CACHED), stage1.t_cached);
    try std.testing.expectEqual(@as(i64, CUM_OUT), stage1.t_out);
    const token_cost1 = stage1.charged - expectedRunFee(s.conn, stage1.wall); // no clamp at BIG_BALANCE
    try std.testing.expectEqual(try expectedTokenCost(s.conn, CUM_IN, CUM_CACHED, CUM_OUT), token_cost1);
    // The registry prices these deltas non-zero — the spec's wire proof arm.
    // `arrange` closed this tenant's trial boundary, so it asserts here
    // unconditionally rather than waiting on a build-time window to lapse.
    try std.testing.expect(token_cost1 > 0);

    // The affinity cursor advanced to the reported cumulatives.
    const cursor = try readAffinityCursor(s.conn);
    try std.testing.expectEqual(@as(i64, CUM_IN), cursor.m_in);
    try std.testing.expectEqual(@as(i64, CUM_CACHED), cursor.m_cached);
    try std.testing.expectEqual(@as(i64, CUM_OUT), cursor.m_out);

    // Re-sent renewal with IDENTICAL cumulatives: token delta is zero — the
    // cumulative-diff idempotency the affinity cursor exists to provide.
    const resp2 = try postRenew(s.h, .{ .input_tokens = CUM_IN, .cached_input_tokens = CUM_CACHED, .output_tokens = CUM_OUT });
    defer resp2.deinit();
    try resp2.expectStatus(.ok);
    const stage2 = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, CUM_IN), stage2.t_in); // accumulated by +0
    try std.testing.expectEqual(@as(i64, CUM_CACHED), stage2.t_cached);
    try std.testing.expectEqual(@as(i64, CUM_OUT), stage2.t_out);
    // The re-send added run time and nothing else: net of the fee its own
    // `wall_ms` prices, the accumulated charge still carries only the first
    // renewal's token cost.
    try std.testing.expectEqual(
        expectedTokenCost(s.conn, CUM_IN, CUM_CACHED, CUM_OUT),
        stage2.charged - expectedRunFee(s.conn, stage2.wall),
    );
    const cursor2 = try readAffinityCursor(s.conn);
    try std.testing.expectEqual(@as(i64, CUM_IN), cursor2.m_in); // unchanged
}

test "integration: wire report settles the final slice from the body's splits and flips the lease reported" {
    // Mid-run cursor: one renewal already metered; the report's final
    // cumulatives settle exactly the remaining diff.
    const s = try arrange(CURSOR_IN, CURSOR_CACHED, CURSOR_OUT);
    defer cleanup(s);

    const report = protocol.ReportRequest{
        .lease_id = LEASE_ID,
        .event_id = EVENT_ID,
        .fencing_token = 1,
        .outcome = .processed,
        .response_text = "wire-splits done",
        .tokens = LEGACY_TOTAL,
        .input_tokens = CUM_IN,
        .cached_input_tokens = CUM_CACHED,
        .output_tokens = CUM_OUT,
        .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = 60_000 },
        .checkpoint = .{ .last_event_id = EVENT_ID, .last_response = "wire-splits done" },
    };
    const body = try std.json.Stringify.valueAlloc(ALLOC, report, .{});
    defer ALLOC.free(body);
    const req = try (try s.h.post(protocol.PATH_RUNNER_REPORTS).bearer(RUNNER_TOKEN)).json(body);
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);

    // The settle slice prices the wire diff: final cumulatives minus the cursor.
    const settle = (try readStage(s.conn)) orelse return error.StageRowMissing;
    try std.testing.expectEqual(@as(i64, SETTLE_D_IN), settle.t_in);
    try std.testing.expectEqual(@as(i64, SETTLE_D_CACHED), settle.t_cached);
    try std.testing.expectEqual(@as(i64, SETTLE_D_OUT), settle.t_out);
    const settle_token_cost = settle.charged - expectedRunFee(s.conn, settle.wall);
    try std.testing.expectEqual(try expectedTokenCost(s.conn, SETTLE_D_IN, SETTLE_D_CACHED, SETTLE_D_OUT), settle_token_cost);
    try std.testing.expect(settle_token_cost > 0);

    // The claim flipped the lease under the fence — the run is settled exactly once.
    try expectLeaseStatus(s.conn, "reported");
}
