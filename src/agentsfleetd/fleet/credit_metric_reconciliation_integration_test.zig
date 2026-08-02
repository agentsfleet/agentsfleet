// Reconciliation between the credit metric and the money Postgres actually
// committed, over the real runner control plane (live DB + Redis).
//
// The credit series is the one OTLP metric that mirrors money. Its correctness
// claim is not "a sample was emitted" but "the samples sum to exactly the debits
// that committed, and nothing else emits". Three emit sites feed it, each firing
// strictly AFTER its own write commits:
//
//   * receive  — service_billing, once per first delivery of an event
//   * renewal  — service_renew, once per successful metered slice
//   * settle   — service_report, once per terminal claim
//
// So the reconciliation reads `billing.usage_ledger`, now the single durable
// home for all three, and compares its total against the drained samples. The
// zero arms matter as much as the sum: a replayed report and a lost fence must
// each contribute nothing, which is only true while the emit stays post-commit.
// Move any of the three emits above its commit and the zero arms below go red.
//
// The sample COUNT no longer reconciles against a row count, and cannot. The
// retired `fleet.metering_periods` appended a row per slice, so four samples
// meant four rows. The ledger accumulates in place under
// `ON CONFLICT (event_id, charge_type)`, so the same four samples leave two
// rows — one `receive`, one `stage` that every renewal and the settle fold
// into. What replaces the count is the stage row's SPAN: `created_at` is the
// first renewal that created it and `last_charged_at` the settle that wrote it
// last, which pins exactly the window the budget apportionment reads.
//
// Free-trial note: the boundary is a tenant fact (§7), so arrange() closes this
// suite's tenant's trial and raises the fixture pair's token rates above zero —
// the reconciliation identity therefore carries real money unconditionally, and
// the strict non-zero arm asserts at any clock position. The zero arms (replay,
// lost fence, failed settle) stay zero for their own reasons, which is the point.
// Requires LIVE_DB + Redis; skipped when either is missing.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const redis_fleet = @import("../queue/redis_fleet.zig");
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");
const otel_metrics = @import("../observability/otel_metrics.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const ChargeType = @import("../state/fleet_telemetry_store.zig").ChargeType;

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8), distinct from every sibling
// suite so cross-test teardown can never race shared rows.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ec011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0eca01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc01";
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "c" ** 64;
const RECON_HOST_ID = "credit-reconciliation-host";
const FLEET_NAME = "credit-reconciliation-bot";

const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;
/// A fencing sequence strictly above the issued lease's token — the exact state
/// a reassignment leaves behind, so the settle claim finds itself superseded.
const SUPERSEDING_FENCING_SEQ: i64 = 99;
const STALE_FENCING_TOKEN: u64 = 1;
const REPORT_WALL_MS: u64 = 100;
const FENCED_ERROR_CODE = "UZ-RUN-005";

// Two renewals reporting monotonically growing cumulative counts, then a final
// report whose cumulatives are the run total. Deltas are what get metered.
const RENEW_ONE_INPUT: u32 = 400;
const RENEW_ONE_CACHED: u32 = 100;
const RENEW_ONE_OUTPUT: u32 = 200;
const RENEW_TWO_INPUT: u32 = 700;
const RENEW_TWO_CACHED: u32 = 250;
const RENEW_TWO_OUTPUT: u32 = 500;
const FINAL_INPUT: u32 = 1000;
const FINAL_CACHED: u32 = 400;
const FINAL_OUTPUT: u32 = 800;

const CONFIG_NO_GATES =
    \\{"name":"credit-reconciliation-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: credit-reconciliation-bot
    \\---
    \\
    \\You are a credit reconciliation test fleet.
;

const METRICS_TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "credit-reconciliation",
    .api_key = "credit-reconciliation",
};

// The real DB-backed runner lookup, parked at module scope so the value
// outlives the middleware chain (tests run sequentially in one process).
// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

// ── Seed + teardown ─────────────────────────────────────────────────────────

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, RECON_HOST_ID, hash[0..] });
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'credit-reconciliation-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
        .event_id = "",
        .fleet_id = FLEET_ID,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

/// Drop the fleet's stream AND its readiness mark — `fleet:ready` is one shared
/// key and `peek` is bounded + randomized, so a leftover mark for a deleted
/// fleet can crowd a sibling suite's fleet out of the sample.
fn forgetFleet(h: *TestHarness) void {
    redis_fleet.purgeFleetRedisState(&h.queue, FLEET_ID) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    forgetFleet(h);
    execIgnore(conn,
        \\DELETE FROM billing.usage_ledger WHERE fleet_id = $1::uuid
    , .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID});
    setFixturePairRates(conn, 0, 0, 0);
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

const Setup = struct { h: *TestHarness, conn: *pg.Conn };

/// Live harness + a fleet with one queued event, ready to be leased. The OTLP
/// metrics exporter is marked installed (no flush thread) so the post-commit
/// record calls enqueue instead of no-oping.
fn arrange() !Setup {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    errdefer h.deinit();
    const conn = try h.acquireConn();
    cleanupAll(h, conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    setFixturePairRates(conn, RATE_INPUT_NANOS_PER_MTOK, RATE_CACHED_NANOS_PER_MTOK, RATE_OUTPUT_NANOS_PER_MTOK);
    try fundLargeBalance(conn);
    // §7: an open trial prices every slice to zero, which would let the
    // reconciliation identity pass by agreeing on nothing. Close this tenant's
    // boundary so the identity carries real money at any clock position.
    try base.endFreeTrialFor(conn, base.TEST_TENANT_ID);
    try seedRunner(conn);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, FLEET_NAME, CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, FLEET_ID, "{}");
    try publishFreshEvent(h);
    otel_metrics.testClear();
    otel_metrics.testSetInstalled(METRICS_TEST_CFG);
    return .{ .h = h, .conn = conn };
}

fn cleanup(s: Setup) void {
    otel_metrics.testClear();
    cleanupAll(s.h, s.conn);
    s.h.releaseConn(s.conn);
    s.h.deinit();
}

// ── Wire helpers ────────────────────────────────────────────────────────────

const LeaseView = struct {
    lease_id: []const u8,
    event_id: []const u8,
    fencing_token: u64,

    fn free(self: LeaseView) void {
        ALLOC.free(self.lease_id);
        ALLOC.free(self.event_id);
    }
};

fn leaseOne(h: *TestHarness) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return error.NoLeaseIssued;
    if (lease == .null) return error.NoLeaseIssued;
    const obj = lease.object;
    const lease_id = try ALLOC.dupe(u8, obj.get("lease_id").?.string);
    errdefer ALLOC.free(lease_id);
    const event_id = try ALLOC.dupe(u8, obj.get("event").?.object.get("event_id").?.string);
    return .{
        .lease_id = lease_id,
        .event_id = event_id,
        .fencing_token = @intCast(obj.get("fencing_token").?.integer),
    };
}

fn renewOnce(h: *TestHarness, lease_id: []const u8, body: protocol.RenewRequest) !void {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const payload = try std.json.Stringify.valueAlloc(ALLOC, body, .{});
    defer ALLOC.free(payload);
    const resp = try (try (try h.post(path).bearer(RUNNER_TOKEN)).json(payload)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
}

fn reportBody(lv: LeaseView, fencing_token: u64) protocol.ReportRequest {
    return .{
        .lease_id = lv.lease_id,
        .event_id = lv.event_id,
        .fencing_token = fencing_token,
        .outcome = .processed,
        .response_text = "done",
        .tokens = FINAL_INPUT + FINAL_CACHED + FINAL_OUTPUT,
        .input_tokens = FINAL_INPUT,
        .cached_input_tokens = FINAL_CACHED,
        .output_tokens = FINAL_OUTPUT,
        .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = REPORT_WALL_MS },
        .checkpoint = .{ .last_event_id = lv.event_id, .last_response = "done" },
    };
}

fn postReport(h: *TestHarness, body: protocol.ReportRequest) !harness_mod.Response {
    const payload = try std.json.Stringify.valueAlloc(ALLOC, body, .{});
    defer ALLOC.free(payload);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(RUNNER_TOKEN)).json(payload);
    return req.send();
}

// ── Reconciliation reads ────────────────────────────────────────────────────

const Emitted = struct { total: i64, count: usize };

/// Drain the exporter ring, keeping only the credit series. Draining is what
/// makes "added zero" checkable: after this returns, the ring is empty, so the
/// next drain sees exactly what the next wire call produced.
fn drainCredit() Emitted {
    var out = Emitted{ .total = 0, .count = 0 };
    while (otel_metrics.testPop()) |sample| {
        if (sample.id != .credit_consumed) continue;
        out.total += sample.value;
        out.count += 1;
    }
    return out;
}

fn scalar(conn: *pg.Conn, sql: []const u8, event_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{event_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoAggregateRow;
    return row.get(i64, 0);
}

/// Every nanocredit Postgres committed for this event — the receive debit plus
/// the accumulated stage charge, both rows of the one table that now owns them.
///
/// The retired shape spread this across `fleet.metering_periods` and
/// `core.fleet_execution_telemetry`, where the renewal accumulated slice
/// charges into BOTH — so summing the second table whole double-counted the
/// same money, and a zero-priced trial hid it. One table, one sum, no mirror.
fn committedDebitTotal(conn: *pg.Conn, event_id: []const u8) !i64 {
    // `SUM(bigint)` is `numeric` in Postgres, so the aggregate is cast back to
    // bigint for the i64 read. The column is bigint, so the cast is lossless.
    return scalar(conn,
        \\SELECT COALESCE(SUM(credit_deducted_nanos), 0)::bigint
        \\FROM billing.usage_ledger WHERE event_id = $1
    , event_id);
}

/// The window the accumulating stage row covers. `created_at` is stamped once,
/// by the write that inserted the row; `last_charged_at` advances on every
/// write after it (`GREATEST(existing, EXCLUDED)`), so the pair brackets the
/// first and last charge without counting how many landed in between.
const LedgerSpan = struct { created_at: i64, last_charged_at: i64 };

fn stageLedgerSpan(conn: *pg.Conn, event_id: []const u8) !LedgerSpan {
    var q = PgQuery.from(try conn.query(
        \\SELECT created_at, last_charged_at FROM billing.usage_ledger
        \\WHERE event_id = $1 AND charge_type = $2
    , .{ event_id, ChargeType.stage.label() }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.StageLedgerRowMissing;
    return .{ .created_at = try row.get(i64, 0), .last_charged_at = try row.get(i64, 1) };
}

// Token rates for the fixture platform pair, raised from the seeded zeros so a
// metered slice prices real money. Restored to zero in cleanupAll — the row
// outlives teardownPlatformProvider (ON CONFLICT DO NOTHING on reseed) and a
// leaked non-zero rate would skew any later suite's exact-charge assertions.
const RATE_INPUT_NANOS_PER_MTOK: i64 = 3_000_000;
const RATE_CACHED_NANOS_PER_MTOK: i64 = 300_000;
const RATE_OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000;

fn setFixturePairRates(conn: *pg.Conn, input: i64, cached: i64, output: i64) void {
    _ = conn.exec(
        \\UPDATE core.model_library SET input_nanos_per_mtok = $3,
        \\  cached_input_nanos_per_mtok = $4, output_nanos_per_mtok = $5
        \\WHERE provider = $1 AND model_id = $2
    , .{ base.TEST_PROVIDER_NAME, base.TEST_PLATFORM_MODEL, input, cached, output }) catch |err|
        std.log.warn("rate set ignored: {s}", .{@errorName(err)});
    model_rate_cache.clear();
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "integration: test_credit_metric_reconciles_committed_debits" {
    const s = try arrange();
    defer cleanup(s);

    // Receive (at lease) + two metered renewals + the terminal settle. The
    // instants around the first renewal and the settle bracket the span the
    // stage row must end up covering.
    const lv = try leaseOne(s.h);
    defer lv.free();
    const before_first_renewal = clock.nowMillis();
    try renewOnce(s.h, lv.lease_id, .{
        .input_tokens = RENEW_ONE_INPUT,
        .cached_input_tokens = RENEW_ONE_CACHED,
        .output_tokens = RENEW_ONE_OUTPUT,
    });
    const after_first_renewal = clock.nowMillis();
    try renewOnce(s.h, lv.lease_id, .{
        .input_tokens = RENEW_TWO_INPUT,
        .cached_input_tokens = RENEW_TWO_CACHED,
        .output_tokens = RENEW_TWO_OUTPUT,
    });
    const before_settle = clock.nowMillis();
    const settle = try postReport(s.h, reportBody(lv, lv.fencing_token));
    defer settle.deinit();
    try settle.expectStatus(.ok);
    const after_settle = clock.nowMillis();

    // The identity: what was emitted is exactly what committed.
    const emitted = drainCredit();
    const committed = try committedDebitTotal(s.conn, lv.event_id);
    try std.testing.expectEqual(committed, emitted.total);

    // And every one of those samples reached the row. A count cannot say so —
    // four samples accumulate into one stage row — but the span can: the row
    // was born in the first renewal and last written by the settle, so a
    // renewal that never committed would leave `created_at` late and a settle
    // that never committed would leave `last_charged_at` back in the renewals.
    const span = try stageLedgerSpan(s.conn, lv.event_id);
    try std.testing.expect(span.created_at >= before_first_renewal);
    try std.testing.expect(span.created_at <= after_first_renewal);
    try std.testing.expect(span.last_charged_at >= before_settle);
    try std.testing.expect(span.last_charged_at <= after_settle);

    // The trial is closed and the pair is rated, so the identity above carries
    // real money rather than agreeing on zero — unconditionally.
    try std.testing.expect(committed > 0);
    try std.testing.expect(emitted.count > 0);

    // Replay: the same report again. The lease is already claimed, so no money
    // moves and no sample may appear.
    const replay = try postReport(s.h, reportBody(lv, lv.fencing_token));
    defer replay.deinit();
    try std.testing.expectEqual(committed, try committedDebitTotal(s.conn, lv.event_id));
    try std.testing.expectEqual(@as(usize, 0), drainCredit().count);
}

test "integration: a settle that loses its fence commits nothing and emits nothing" {
    const s = try arrange();
    defer cleanup(s);

    const lv = try leaseOne(s.h);
    defer lv.free();
    _ = drainCredit(); // discard the receive debit; this arm is about the settle

    // Another holder took the fleet: affinity's sequence now outruns this
    // lease's token, so the settle statement finds itself superseded.
    _ = try s.conn.exec(
        "UPDATE fleet.runner_affinity SET fencing_seq = $1 WHERE fleet_id = $2::uuid",
        .{ SUPERSEDING_FENCING_SEQ, FLEET_ID },
    );
    const before = try committedDebitTotal(s.conn, lv.event_id);

    const resp = try postReport(s.h, reportBody(lv, STALE_FENCING_TOKEN));
    defer resp.deinit();
    try resp.expectErrorCode(FENCED_ERROR_CODE);

    try std.testing.expectEqual(before, try committedDebitTotal(s.conn, lv.event_id));
    try std.testing.expectEqual(@as(usize, 0), drainCredit().count);
}

// A test named "a settle whose database write fails emits nothing" stood here.
// It forced the failure by occupying the (event_id, slice_seq) slot the settle
// was about to write, which raised only because `fleet.metering_periods` had no
// ON CONFLICT clause. Every write that replaced it arbitrates its own conflict,
// so the seam is gone and the settle path has no fault injection by design.
// Dropped rather than faked; the emit-after-commit ordering it guarded is still
// covered on the fence-loss and replay arms above, both of which also require a
// settle that moves no money to emit nothing.
