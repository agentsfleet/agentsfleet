// Integration test for the `/renew` HTTP route's service-layer credit gate
// (`service_renew.renew` -> `metering.balanceCoversEstimate`). The SQL-core
// renewal tests (`renewal_integration_test.zig`) drive `renewal.renew` directly,
// which deliberately does NOT credit-gate, so the broke-tenant refusal is only
// reachable through the handler. This drives the real router + runner_bearer
// middleware against the live test DB: an exhausted tenant's renewal is refused
// with UZ-RUN-012 and the lease's kill deadline is left untouched (a broke
// tenant's run ends at its original deadline, never extended).
//
// The gate only refuses under the .stop balance policy (now the default), so the
// test sets ctx.balance_policy = .stop on the harness directly (explicit, so the
// assertion is independent of the configured default).
// Requires LIVE_DB=1; skipped when TEST_DATABASE_URL is unset. The refusal
// needs a non-zero stage charge to be a refusal at all, so the suite seeds a
// priced catalogue row for the lease's (provider, model) pair. Without the rate
// row the gate hits `error.ModelNotPriced` and fails OPEN by design, which
// reads as "the tenant could pay".

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
const tenant_billing = @import("../state/tenant_billing.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8c01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8f01";
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "c" ** 64;
// The lease's own (provider, model) pair, and the catalogue row that prices it.
// The estimate floor is ESTIMATE_FLOOR_{INPUT,OUTPUT}_TOKENS priced at these
// rates — a few thousand nanos, comfortably above the zero balance the gate
// weighs it against, and far below any grant, so the verdict turns on the
// balance and nothing else.
const PRICED_MODEL_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8d01";
const LEASE_PROVIDER = "test-provider";
const LEASE_MODEL = "test-model";
/// Any positive rate refuses a zero balance; this one keeps the slice well
/// clear of integer-rounding to zero at the token counts reported below.
const PRICE_NANOS_PER_MTOK: i64 = 1_000_000_000;
/// Reported cumulative usage. The renewal charges the DELTA against the
/// affinity cursors (seeded at zero), so these are the whole slice.
const RENEW_BODY = "{\"input_tokens\":500000,\"cached_input_tokens\":0,\"output_tokens\":500000}";

// The real DB-backed runner lookup, parked at module scope so the value outlives
// the middleware chain (tests run sequentially in one process).
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
        \\VALUES ($1::uuid, 'renew-credit-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

fn seedActiveLease(conn: *pg.Conn, lease_expires_at: i64) !void {
    // The affinity slot the renewal probe INNER JOINs on, and where the metering
    // cursors now live. Without it the probe matches no row, so the renewal
    // reports `lost` (UZ-RUN-011) and never reaches the credit gate this test is
    // about. `fencing_seq` matches the lease's token so the fence holds.
    //
    // The lease's `created_at` is NOW, not 0: the renewal guard caps a run at
    // `created_at + MAX_RUNTIME_MS`, so an epoch-zero lease is already past its
    // hard cap and is refused with UZ-RUN-010 before the credit gate is reached.
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE
        \\  SET fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ FLEET_ID, RUNNER_ID, lease_expires_at });

    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-renew-credit-1',
        \\        'steer:test', 'chat', 0, 'platform',
        \\        $7, $8, 0, 0, 0, 0, 1, $6, 'active', $9, $9)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, lease_expires_at, LEASE_PROVIDER, LEASE_MODEL, clock.nowMillis() });
}

// Seed a PRESENT billing row at zero balance: balanceCoversEstimate reads a real
// 0 — not a missing-row fail-open, which would cover — so under the .stop policy
// the gate refuses the renewal charge.
fn exhaustBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, 0, 'renew-credit-exhaust', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE SET balance_nanos = EXCLUDED.balance_nanos
    , .{base.TEST_TENANT_ID});
}

fn leaseExpiresAtOf(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query("SELECT lease_expires_at FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return row.get(i64, 0);
}

/// Price the lease's (provider, model) so a renewal slice costs something.
///
/// Migrations install NO catalogue — those rows exist only after the seed test
/// applies `seed.sql`, so reading them here would make a billing invariant
/// depend on suite ordering. The fixture therefore seeds its own row, the same
/// shape `model_catalogue_revision_integration_test` uses. Without it the slice
/// prices at zero, a zero balance trivially "covers" it, and the renewal the
/// credit gate is supposed to refuse succeeds with 200.
fn seedPricedModel(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 128000, $4, 0, $4, 0, 0)
        \\ON CONFLICT (provider, model_id) DO UPDATE SET
        \\   input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        \\   output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok
    , .{ PRICED_MODEL_ID, LEASE_MODEL, LEASE_PROVIDER, PRICE_NANOS_PER_MTOK });
}

fn renewLease(h: *TestHarness) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, LEASE_ID, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(RUNNER_TOKEN)).json(RENEW_BODY);
    return req.send();
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    // Drop this suite's catalogue row and clear the process-global rate cache;
    // without the clear the entry outlives its row and a later suite resolves a
    // price for a pair the database no longer carries.
    _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1 AND model_id = $2", .{ LEASE_PROVIDER, LEASE_MODEL }) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    model_rate_cache.clear();
    base.teardownTenant(conn);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    // Last: the ledger rows referencing this catalogue row are gone with the
    // tenant above, so the delete-guard foreign key no longer restricts it.
    _ = conn.exec("DELETE FROM core.model_library WHERE id = $1::uuid", .{PRICED_MODEL_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

test "integration: renew refused with UZ-RUN-012 on an exhausted tenant, deadline left untouched" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();

    // The credit gate only refuses under .stop, which is now the production
    // default (balance_policy.DEFAULT), so the harness Context already carries it.
    // The explicit set keeps this test independent of the configured default —
    // the gate reads ctx.balance_policy, so no env mutation is needed.
    h.ctx.balance_policy = .stop;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try seedPricedModel(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "service-renew-fleet", "{}", "# z");
    try seedRunner(conn);
    try exhaustBalance(conn); // 0 balance → under .stop the gate refuses the renewal charge
    const deadline = clock.nowMillis() + 60_000;
    try seedActiveLease(conn, deadline);
    defer teardown(conn);

    // The charge is priced: the tenant is genuinely broke against a real
    // estimate, which is the only state this refusal is about.
    const billing = (try tenant_billing.getBilling(conn, ALLOC, base.TEST_TENANT_ID)).?;
    defer ALLOC.free(@constCast(billing.grant_source));
    try std.testing.expectEqual(@as(i64, 0), billing.balance_nanos);

    // Credit gate sits after the ownership + active-status checks, so an owned,
    // active lease reaches it; the broke tenant is refused.
    const before = try leaseExpiresAtOf(conn);
    const resp = try renewLease(h);
    defer resp.deinit();
    try resp.expectStatus(.payment_required); // 402 — refusal must carry the right status, not just the code
    try resp.expectErrorCode("UZ-RUN-012"); // lease_renewal_no_credits

    // A refused renewal must never advance the kill deadline.
    try std.testing.expectEqual(before, try leaseExpiresAtOf(conn));
}

test "integration: a transient DB fault loading the lease is a retryable 5xx, not a terminal 404" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "service-renew-fleet", "{}", "# z");
    try seedRunner(conn);
    try seedActiveLease(conn, clock.nowMillis() + 60_000);
    defer teardown(conn);

    // Inject a transient-style DB fault on a VALID, owned, active lease: rename a
    // column the load query selects so the SELECT errors. DDL autocommits, so the
    // handler's pooled connection sees it on its next parse. The fix must surface
    // this as a retryable 5xx (the runner renews again next tick) — never a
    // terminal 404 that would make it kill a healthy long-running child.
    _ = try conn.exec("ALTER TABLE fleet.runner_leases RENAME COLUMN status TO status_faultinj", .{});
    // Backstop restore if the immediate restore below is skipped (send errored).
    defer _ = conn.exec("ALTER TABLE fleet.runner_leases RENAME COLUMN status_faultinj TO status", .{}) catch {};

    const resp = try renewLease(h);
    // Restore before any assertion can early-return, so the rest of the suite
    // sees the original schema (the backstop defer above then no-ops).
    _ = conn.exec("ALTER TABLE fleet.runner_leases RENAME COLUMN status_faultinj TO status", .{}) catch {};
    defer resp.deinit();

    try resp.expectStatus(.internal_server_error); // 5xx, retryable — not a terminal 404
    try resp.expectErrorCode("UZ-INTERNAL-002");
}

test "integration: a malformed lease_id is a terminal 404, never a retryable 5xx" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    defer teardown(conn);

    // A non-UUID lease_id can never match a lease. The uuidv7 gate rejects it as
    // not-found BEFORE the query, so the ::uuid cast is never the error source —
    // the runner gets a terminal 404, never a 5xx that would make it spin
    // retrying a lease that cannot exist.
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, "not-a-uuid", protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();

    try resp.expectStatus(.not_found);
    try resp.expectErrorCode("UZ-RUN-006");
}
