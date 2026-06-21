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
// Requires LIVE_DB=1; skipped when TEST_DATABASE_URL is unset, and also skipped
// while the free-trial window is open (stage charge is 0, so the gate can't
// refuse — see the free-trial section in billing_and_provider_keys.md). Asserts
// live post-trial.

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

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8c01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d8f01";
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "c" ** 64;

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
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-renew-credit-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, 1, $6, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, lease_expires_at });
}

// Seed a PRESENT billing row at zero balance: balanceCoversEstimate reads a real
// 0 — not a missing-row fail-open, which would cover — so under the .stop policy
// the gate refuses the renewal charge.
fn exhaustBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
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

fn renewLease(h: *TestHarness) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, LEASE_ID, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(RUNNER_TOKEN)).json("{}");
    return req.send();
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    base.teardownTenant(conn);
    base.teardownWorkspace(conn, WORKSPACE_ID);
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
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    try exhaustBalance(conn); // 0 balance → under .stop the gate refuses the renewal charge
    const deadline = clock.nowMillis() + 60_000;
    try seedActiveLease(conn, deadline);
    defer teardown(conn);

    // While the free-trial window is open, stage charge is 0 for every posture,
    // so a 0 balance still "covers" and the gate cannot refuse — the same skip
    // the metering gate tests use (see the free-trial section in
    // billing_and_provider_keys.md). Asserted live once now_ms passes FREE_TRIAL_END_MS.
    const trial_active = blk: {
        const b = (try tenant_billing.getBilling(conn, ALLOC, base.TEST_TENANT_ID)).?;
        defer ALLOC.free(@constCast(b.grant_source));
        break :blk b.free_trial_active;
    };
    if (trial_active) return error.SkipZigTest;

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
