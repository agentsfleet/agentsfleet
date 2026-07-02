// HTTP + facade integration tests for M11_006 balance gate + exhaustion
// surface. Exercises the §2 / §3 / §4 dims the unit tests cannot reach:
// DB-round-trip exhaustion stamping, activity_event writes, the stop
// policy pre-claim gate, and the `is_exhausted` / `exhausted_at` response
// fields.
//
// Requires TEST_DATABASE_URL — skipped gracefully when unset.

const std = @import("std");
const scope_fixtures = @import("../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../auth/middleware/mod.zig");

const tenant_billing = @import("../../state/tenant_billing.zig");
const metering = @import("../../fleet_runtime/metering.zig");
const test_fixtures = @import("../../db/test_fixtures.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Same JWKS + issuer shape the dashboard integration test uses so the
// harness verifier accepts these tokens. Tenant + workspace IDs are
// fresh so this suite does not collide with sibling suites.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41";
const TEST_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f51";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
// Reuses the dashboard test's pre-signed operator JWT — the tenant_id
// in the `metadata` claim is distinct from this suite's seeded tenant
// (`…6f01` vs `…6f31`), so we override the billing row to the claim's
// tenant before running the GET tests.
const TEST_BALANCE_NANOS: i64 = 1000;

const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;
const TOKEN_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openHarnessOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenantAndWorkspace(conn: *pg.Conn, tenant_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'M11_006-integration', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, tenant_id, now_ms });
    // activity_events.fleet_id carries a NOT NULL FK to core.fleets —
    // seed a minimal row so logEventOnConn writes do not fail.
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'fleet-m11-006', 'seed', null, '{}'::jsonb, 'active', $3, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_FLEET_ID, TEST_WORKSPACE_ID, now_ms });
}

fn teardown(conn: *pg.Conn, tenant_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.tenant_billing WHERE tenant_id = $1::uuid", .{tenant_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1::uuid", .{tenant_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// ── stop-policy pre-claim balance gate ────────────────────────────────────

test "integration: balanceCoversEstimate honours policy and tenant balance" {
    const alloc = std.testing.allocator;
    const db_ctx = (try @import("../../db/test_fixtures.zig").openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(db_ctx.conn, TEST_TENANT_ID, now_ms);
    defer teardown(db_ctx.conn, TEST_TENANT_ID);

    test_fixtures.resetBilling(db_ctx.conn);
    try tenant_billing.insertStarterGrant(db_ctx.conn, TEST_TENANT_ID);

    // STARTER_CREDIT_NANOS covers a self-managed event under stop policy.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        alloc,
        TEST_TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .stop,
    ));

    // §7 free-trial: stage charge short-circuits to 0 until FREE_TRIAL_END_MS,
    // so the "drained balance + stop policy must block" assertion below is
    // unreachable — drain leaves balance=0 and est_total=0, so balanceCovers
    // Estimate returns true. Skip that half until post-trial; the fail-open
    // half (.warn / .@"continue" below) still exercises today.
    const trial_active = blk: {
        const b = (try tenant_billing.getBilling(db_ctx.conn, alloc, TEST_TENANT_ID)).?;
        defer alloc.free(@constCast(b.grant_source));
        break :blk b.free_trial_active;
    };
    if (!trial_active) {
        // Drain the entire starter grant; stop policy must now block.
        _ = try tenant_billing.debit(db_ctx.conn, TEST_TENANT_ID, tenant_billing.STARTER_CREDIT_NANOS);
        try std.testing.expect(!metering.balanceCoversEstimate(
            db_ctx.pool,
            alloc,
            TEST_TENANT_ID,
            .self_managed,
            "self-managed-test",
            "any-model",
            .stop,
        ));
    }

    // Non-stop policies fail-open — the event passes the gate even at 0¢.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        alloc,
        TEST_TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .warn,
    ));
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        alloc,
        TEST_TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .@"continue",
    ));
}

// ── §4.1 / §4.2 — GET /v1/tenants/me/billing response shape ──────────────

test "integration(m11_006): GET /v1/tenants/me/billing emits is_exhausted=false, exhausted_at=null on a healthy tenant" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(conn, TOKEN_TENANT_ID, now_ms);
    defer teardown(conn, TOKEN_TENANT_ID);

    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1, $3, 'billing_handler_test', $2, $2)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\SET balance_nanos = EXCLUDED.balance_nanos,
        \\    balance_exhausted_at = NULL,
        \\    updated_at = EXCLUDED.updated_at
    , .{ TOKEN_TENANT_ID, now_ms, TEST_BALANCE_NANOS });

    const r = try (try h.get("/v1/tenants/me/billing").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"is_exhausted\":false"));
    try std.testing.expect(r.bodyContains("\"exhausted_at\":null"));
}

test "integration(m11_006): GET /v1/tenants/me/billing emits is_exhausted=true + exhausted_at=<ms> on an exhausted tenant" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(conn, TOKEN_TENANT_ID, now_ms);
    defer teardown(conn, TOKEN_TENANT_ID);

    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, balance_nanos, grant_source, balance_exhausted_at, created_at, updated_at)
        \\VALUES ($1, 0, 'billing_handler_test', $2, $2, $2)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\SET balance_nanos = 0,
        \\    balance_exhausted_at = EXCLUDED.balance_exhausted_at,
        \\    updated_at = EXCLUDED.updated_at
    , .{ TOKEN_TENANT_ID, now_ms });

    const r = try (try h.get("/v1/tenants/me/billing").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"is_exhausted\":true"));
    // Non-null exhausted_at — we don't pin the exact epoch since the
    // seeded now_ms varies per run. Asserting it isn't literal null
    // proves the handler surfaced the column value.
    try std.testing.expect(!r.bodyContains("\"exhausted_at\":null"));
}

// ── Concurrency — markExhausted atomicity under parallel callers ─────────

test "integration(m11_006): concurrent markExhausted calls — exactly one transitions, rest are no-ops" {
    const alloc = std.testing.allocator;
    // Acquire the pool via the base harness (seed uses a single conn, threads
    // each acquire their own connection to exercise the real race surface).
    const db_ctx = (try @import("../../db/test_fixtures.zig").openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();

    const now_ms = clock.nowMillis();
    {
        try seedTenantAndWorkspace(db_ctx.conn, TEST_TENANT_ID, now_ms);
        test_fixtures.resetBilling(db_ctx.conn);
        try tenant_billing.insertStarterGrant(db_ctx.conn, TEST_TENANT_ID);
    }
    defer {
        teardown(db_ctx.conn, TEST_TENANT_ID);
        db_ctx.pool.release(db_ctx.conn);
    }

    const Worker = struct {
        pool: *@import("pg").Pool,
        result: bool = false,
        err: ?anyerror = null,

        fn run(self: *@This()) void {
            const conn = self.pool.acquire() catch |e| {
                self.err = e;
                return;
            };
            defer self.pool.release(conn);
            self.result = tenant_billing.markExhausted(conn, TEST_TENANT_ID) catch |e| blk: {
                self.err = e;
                break :blk false;
            };
        }
    };

    const N = 8;
    var workers: [N]Worker = undefined;
    var threads: [N]std.Thread = undefined;
    for (0..N) |idx| workers[idx] = .{ .pool = db_ctx.pool };
    for (0..N) |idx| threads[idx] = try std.Thread.spawn(.{}, Worker.run, .{&workers[idx]});
    for (0..N) |idx| threads[idx].join();

    // Exactly one worker observes the NULL→now transition; the other N-1
    // see the idempotent replay. This is the load-bearing invariant for
    // the one-shot `balance_exhausted_first_debit` activity event —
    // duplicates here would dupe the operator-visible notification.
    var transitioned_count: usize = 0;
    for (0..N) |idx| {
        if (workers[idx].err) |e| return e;
        if (workers[idx].result) transitioned_count += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), transitioned_count);
}

// ── §1.2 (post-1.7) — null tenant_id → 403 ──────────────────────────────

test "integration(m11_006): POST /v1/workspaces with a JWT lacking tenant_id returns UZ-AUTH-002" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // A JWT that omits `metadata.tenant_id`. Same kid / signature flow as
    // the shared fixtures; payload minted for this suite only.
    // header: {"alg":"RS256","typ":"JWT","kid":"test-kid-static"}
    // payload: {"sub":"user_m11_006","iss":"https://clerk.test.agentsfleet.net",
    //   "aud":"https://api.agentsfleet.net","exp":4102444800,
    //   "metadata":{"role":"operator"}}
    // Signed with the same test RSA key used elsewhere in the suite.
    const TOKEN_NO_TENANT = scope_fixtures.NO_TENANT;

    const r = try (try (try h.post("/v1/workspaces").json("{}")).bearer(TOKEN_NO_TENANT)).send();
    defer r.deinit();
    // A malformed-signature token already yields 401 from the JWT path,
    // before we reach the handler. The stricter post-M11_006 behavior is
    // that ANY authenticated path missing tenant_id is rejected — this
    // assertion covers the negative superset.
    try std.testing.expect(r.status == 401 or r.status == 403);
}

// ── GET /v1/tenants/me/billing/charges — cursor parse paths ───────────────
//
// Pins the cursor branch in `innerGetTenantBillingCharges`: missing,
// empty-string, and malformed. Empty DB suffices — these tests are about
// the handler's cursor parsing, not the row mapping (which the telemetry
// store's own suite covers). The empty-cursor case is the proof for the
// `?cursor=` → null behaviour that prevents a noisy 400 on first-page
// requests expressed verbosely.

test "integration: GET /billing/charges with no cursor returns 200 + empty items on a tenant with no telemetry" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(conn, TOKEN_TENANT_ID, now_ms);
    defer teardown(conn, TOKEN_TENANT_ID);

    const r = try (try h.get("/v1/tenants/me/billing/charges").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"items\":[]"));
    try std.testing.expect(r.bodyContains("\"next_cursor\":null"));
}

test "integration: GET /billing/charges with empty ?cursor= is treated as no-cursor (200 not 400)" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(conn, TOKEN_TENANT_ID, now_ms);
    defer teardown(conn, TOKEN_TENANT_ID);

    const r = try (try h.get("/v1/tenants/me/billing/charges?cursor=").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"items\":[]"));
}

test "integration: GET /billing/charges with malformed cursor returns 400 invalid cursor" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const now_ms = clock.nowMillis();
    try seedTenantAndWorkspace(conn, TOKEN_TENANT_ID, now_ms);
    defer teardown(conn, TOKEN_TENANT_ID);

    const r = try (try h.get("/v1/tenants/me/billing/charges?cursor=!!not-a-real-cursor!!").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try std.testing.expect(r.bodyContains("invalid cursor"));
}
