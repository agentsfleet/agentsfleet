// Runner-enrollment authz over the live HTTP surface: `POST
// /v1/runners` mints a `agt_r` only for a verified JWT whose `scopes` claim
// carries `runner:enroll`; a tenant-scoped JWT (no `runner:enroll`) and a
// `agt_t` api_key are both rejected `403 UZ-AUTH-022`.
//
// The DB-backed arms require TEST_DATABASE_URL — skipped gracefully otherwise
// via `TestHarness.start` returning `error.SkipZigTest`. The first test needs
// no DB: it drives the real `oidc.Verifier` against the inline JWKS to prove
// the fixture token actually verifies (RS256 signature + claim extraction)
// through production code, not just that it was signed correctly.
//
// Fixtures (JWKS + the two tokens) are generated offline with an RSA keypair we
// do not commit; regenerate with the Node script in this PR's Session Notes.
// Payload shape mirrors the Clerk session token: top-level `scopes` (the
// capability claim) + `metadata.{tenant_id, workspace_id}`. `exp` is 4102444800
// (2100) so the fixture never ages out.

const std = @import("std");
const scope_fixtures = @import("./test_scope_tokens.zig");
const clock = @import("common").clock;
const auth_mw = @import("../auth/middleware/mod.zig");
const oidc = @import("../auth/oidc.zig");
const api_key = @import("../auth/api_key.zig");
const api_key_lookup = @import("../cmd/api_key_lookup.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const error_registry = @import("../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECK passes.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const API_KEY_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001";

// A valid tenant api_key. The DB stores only its SHA-256 hash; a `agt_t`
// carries the workspace_admin tenant-scope bundle but never `runner:enroll`.
const AGT_T_KEY = auth_mw.tenant_api_key.TENANT_KEY_PREFIX ++ "c" ** 48;

const REGISTER_BODY =
    \\{"host_id":"host-enroll-test","sandbox_tier":"dev_none","labels":[]}
;

const TEST_JWKS = scope_fixtures.JWKS;
// scopes claim carries `runner:enroll` → may enroll a runner.
const OPERATOR_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX29wX20xMDQiLCJpc3MiOiJodHRwczovL2NsZXJrLnRlc3QuYWdlbnRzZmxlZXQubmV0IiwiYXVkIjoiaHR0cHM6Ly9hcGkuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwLCJzY29wZXMiOiJydW5uZXI6ZW5yb2xsIHJ1bm5lcjpyZWFkIHJ1bm5lcjp3cml0ZSBzdHJlYW06cmVhZCIsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSJ9fQ.ADYOJzBdVCEgAppXcvxCdOPN7yv3xCf44D2tFCDrWV9OASGC5ohfehxYCHXMz-rrmsq48lWHvpLt9R0qX99QDo4oBWjBd4c8-CcRl3KnWsNNTQVd7HeZNYcY3KdwDx8aXxdAr_Hsu6dHhGy2SeEFzlYffbtZT-qr6VT2nAqjNomcvUaPfNqb2U2-KwlS18ypM3VBxU2LYDMsjoRzxcex-ZgM49wzMccTFRKLJkPgmC0YI5i4BzmnYFDZ6owfjzVacHls_tNErBPQJ_gPbSl42eM_i-xqvtImdz8j9VJewbQ0CSAFc_PcdpNuHLDnOYfXSksFjnE5wbk5D0xlRBvrkQ";
// scopes carry tenant capability only (no `runner:enroll`) → must be 403.
const TENANT_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3RlbmFudF9tMTA0IiwiaXNzIjoiaHR0cHM6Ly9jbGVyay50ZXN0LmFnZW50c2ZsZWV0Lm5ldCIsImF1ZCI6Imh0dHBzOi8vYXBpLmFnZW50c2ZsZWV0Lm5ldCIsImV4cCI6NDEwMjQ0NDgwMCwic2NvcGVzIjoiZmxlZXQ6YWRtaW4gY3JlZGVudGlhbDp3cml0ZSIsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSJ9fQ.LHVx2-AWzyGF4uPeAzBTUVQHrPrRrQMne8-x9w0N0S-S5n7cYzB663wdlfuPAysmxw9vy022UYvJ0pSIKeMgiydIvWq5_WRu7vvOJm7wsnrIRPw13vQEB3Emk0JWYJzD0TRAajy6AcJTD2-R5OZfCLS4Es3iKl2GsGLm86NJbF6RkgBm4MtdfVkKo_K9vvSwqem3by8p-urKoGrilsd1RBQv6nOvAKyQIhHptLvRRNK1DC3bKuMWsUtuhbdEaEGBFmwYdBl7DIZHlH2rDkZ4d5iUQ_uMbRyiqzPKenUzJUj0HPbdo4N5y5EA2jo6XKw-AOkSHVkNCMZYSl4zz2OKmg";

// ── Verifier proof (no DB) ───────────────────────────────────────────────────
// Drives the real oidc.Verifier so the fixture is proven against production
// signature + claim-extraction code, independent of the register handler / DB.

fn freePrincipal(p: oidc.Principal) void {
    ALLOC.free(p.subject);
    ALLOC.free(p.issuer);
    if (p.tenant_id) |v| ALLOC.free(v);
    if (p.org_id) |v| ALLOC.free(v);
    if (p.workspace_id) |v| ALLOC.free(v);
    if (p.audience) |v| ALLOC.free(v);
    if (p.scopes) |v| ALLOC.free(v);
}

fn verify(token: []const u8) !oidc.Principal {
    var verifier = oidc.Verifier.init(ALLOC, .{
        .jwks_url = "https://test.invalid/jwks",
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
        .inline_jwks_json = TEST_JWKS,
    });
    defer verifier.deinit();
    const auth = try std.fmt.allocPrint(ALLOC, "Bearer {s}", .{token});
    defer ALLOC.free(auth);
    return verifier.verifyAuthorization(ALLOC, auth);
}

test "fixture tokens verify through the real oidc verifier; scopes claim surfaces" {
    const operator = try verify(OPERATOR_TOKEN);
    defer freePrincipal(operator);
    // The operator fixture carries the platform-plane scopes incl. runner:enroll.
    try std.testing.expectEqualStrings("runner:enroll runner:read runner:write stream:read", operator.scopes.?);

    const tenant = try verify(TENANT_TOKEN);
    defer freePrincipal(tenant);
    // The tenant fixture carries tenant capability only — no runner:enroll.
    try std.testing.expectEqualStrings("fleet:admin credential:write", tenant.scopes.?);
}

// ── Register-handler authz (DB-backed) ───────────────────────────────────────

// SAFETY: populated by configureRegistry (with the harness pool) before the
// middleware chain — and thus the lookup — ever reads it.
var api_key_ctx: api_key_lookup.Ctx = undefined;
// SAFETY: populated by configureRegistry (with the harness pool) before the
// runner-bearer middleware — and thus the lookup — ever reads it. Wired so a
// minted `agt_r` resolves against `fleet.runners` (the harness default uses a
// null stub).
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    api_key_ctx = .{ .pool = h.pool };
    reg.tenant_api_key_mw = .{ .host = &api_key_ctx, .lookup = api_key_lookup.lookup };
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenantAndApiKey(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'Runner Enroll Test Tenant', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TENANT_ID, now_ms });
    const key_hash = api_key.sha256Hex(AGT_T_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys (uid, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'runner-enroll-test-key', '', $3, 'user_enroll_test', TRUE, $4, $4)
        \\ON CONFLICT (key_hash) DO NOTHING
    , .{ API_KEY_ROW_ID, TENANT_ID, key_hash[0..], now_ms });
}

fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.api_keys WHERE uid = $1::uuid", .{API_KEY_ROW_ID}) catch |err|
        std.log.warn("cleanup api_keys ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id = 'host-enroll-test'", .{}) catch |err|
        std.log.warn("cleanup runners ignored: {s}", .{@errorName(err)});
}

test "register: a runner:enroll JWT mints a agt_r (201)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(OPERATOR_TOKEN)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
    try std.testing.expect(resp.bodyContains(auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX));
}

test "register: a tenant-scoped JWT without runner:enroll is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(TENANT_TOKEN)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_INSUFFICIENT_SCOPE);
}

test "register: a agt_t api_key cannot enroll a runner (403)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(AGT_T_KEY)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_INSUFFICIENT_SCOPE);
}

test "register: the mint records last_seen_at = 0 (never connected → registered)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const mint = try (try (try h.post(protocol.PATH_RUNNERS).bearer(OPERATOR_TOKEN)).json(REGISTER_BODY)).send();
    defer mint.deinit();
    try mint.expectStatus(.created);

    // The row carries the never-seen sentinel, so the fleet read derives
    // `registered` (not a fake `online`) until the first heartbeat moves it.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT last_seen_at FROM fleet.runners WHERE host_id = 'host-enroll-test'", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(protocol.RUNNER_LAST_SEEN_NEVER, try row.get(i64, 0));
}

// ── Operator-plane fleet read (GET /v1/fleets/runners) ────────────────────────
// Same platform-admin gate as enrollment; read-only; derives liveness and never
// leaks the token hash or the raw agt_r.

test "fleet list: a runner:read JWT lists the fleet with derived liveness (200)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const mint = try (try (try h.post(protocol.PATH_RUNNERS).bearer(OPERATOR_TOKEN)).json(REGISTER_BODY)).send();
    defer mint.deinit();
    try mint.expectStatus(.created);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(OPERATOR_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("host-enroll-test"));
    try std.testing.expect(resp.bodyContains("registered")); // never-connected liveness
    try std.testing.expect(resp.bodyContains("\"admin_state\":\"active\""));
    try std.testing.expect(!resp.bodyContains("token_hash")); // invariant: hash never leaves
    try std.testing.expect(!resp.bodyContains(auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX)); // the raw token is mint-only
}

test "fleet list: a tenant-scoped JWT without runner:read is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(TENANT_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_INSUFFICIENT_SCOPE);
}

test "fleet list: a agt_t api_key is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(AGT_T_KEY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_INSUFFICIENT_SCOPE);
}

// ── Runner-plane auth gate: admin_state admits only `active` ──────────────────
// The runnerBearer lookup (serve_runner_lookup) gates on `admin_state == active`,
// so a revoked/cordoned runner's token is rejected at the middleware before any
// `/v1/runners/me/*` handler runs. (The end-to-end PATCH-revoke → 401 flow is the
// operator-plane mutation's own test; here the gate is proven by flipping the
// stored admin_state directly.)

// UUIDv7 (version nibble 7) so the schema id CHECK passes; tenant_id NULL = trusted fleet.
const GATE_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002";
const GATE_RAW_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "g" ** 60;

fn setGateRunner(h: *TestHarness, admin_state: []const u8) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const hash = api_key.sha256Hex(GATE_RAW_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'host-gate-test', $2, 'dev_none', $3, '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET admin_state = EXCLUDED.admin_state
    , .{ GATE_RUNNER_ID, hash[0..], admin_state });
}

fn cleanupGate(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{GATE_RUNNER_ID}) catch |err|
        std.log.warn("cleanup gate runner ignored: {s}", .{@errorName(err)});
}

test "runner auth admits an active admin_state and rejects a revoked one" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanupGate(h);

    // active → the runner plane admits (GET /v1/runners/me → 200).
    try setGateRunner(h, @tagName(protocol.AdminState.active));
    {
        const resp = try (try h.get(protocol.PATH_RUNNER_SELF).bearer(GATE_RAW_TOKEN)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }

    // revoked → the same token is rejected at the middleware (401), before /me runs.
    try setGateRunner(h, @tagName(protocol.AdminState.revoked));
    {
        const resp = try (try h.get(protocol.PATH_RUNNER_SELF).bearer(GATE_RAW_TOKEN)).send();
        defer resp.deinit();
        try resp.expectStatus(.unauthorized);
    }
}

// Enrollment is mint-by-API only: the `agentsfleet-runner register` CLI was retired,
// so there is no binary-spawned register arm. The handler authz above is the
// enrollment contract; the `agt_r` is minted server-side from the dashboard's
// session-authed POST (proven here directly against the live HTTP surface).
