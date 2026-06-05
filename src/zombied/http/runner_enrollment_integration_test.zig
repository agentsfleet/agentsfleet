// Runner-enrollment authz over the live HTTP surface: `POST /v1/runners` mints
// a `zrn_` only for a verified JWT carrying `metadata.platform_admin == true`;
// a tenant-admin JWT and a `zmb_t_` api_key are both rejected `403`.
//
// The DB-backed arms require TEST_DATABASE_URL — skipped gracefully otherwise
// via `TestHarness.start` returning `error.SkipZigTest`. The first test needs
// no DB: it drives the real `oidc.Verifier` against the inline JWKS to prove
// the fixture token actually verifies (RS256 signature + claim extraction)
// through production code, not just that it was signed correctly.
//
// Fixtures (JWKS + the two tokens) are generated offline with an RSA keypair we
// do not commit; regenerate with the script in this PR's Session Notes. Payload
// shape mirrors the Clerk session token: `metadata.{tenant_id, role,
// platform_admin}`. `exp` is 4102444800 (2100) so the fixture never ages out.

const std = @import("std");
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

const TEST_ISSUER = "https://clerk.test.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECK passes.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const API_KEY_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001";

// A valid tenant api_key. The DB stores only its SHA-256 hash; a `zmb_t_`
// authenticates as `.role=.admin` but never carries `platform_admin`.
const ZMB_T_KEY = "zmb_t_" ++ "c" ** 48;

const REGISTER_BODY =
    \\{"host_id":"host-enroll-test","sandbox_tier":"dev_none","labels":[]}
;

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"0Z8ud27-1vd_WsxIcCdMkFeWNiGYpOIKhKAkQruCx6lIzCiDnKyH4I1fL2copGyb5EXdzmqrPvMIKEvoSXGUafrjWp8QneMKdVXoFwRsdrsaEcXg_1npJuiF9smRouTn8pda6m0bwcjn8jBXdBo4q_Eah9O03A8yrC-ZfNqDKjClG0lsYWlJVxpcUIYGQNNVI6LRhYD3tQnzu_4vQdW_FgDrPffwv2uA6YQoMt-Tq93LtDZFE8PlEW43vDcSRw-1gWQazcLw9VPEw6vAywE7PLeQyx3cjIQZxBDo0eDld4J6oprxatCVZ0I-CuBdj07PvGFYmWke5nfV-zsbwwwvhw","e":"AQAB","kid":"m80005-test-kid","use":"sig","alg":"RS256"}]}
;
// metadata.platform_admin == true → may enroll.
const PLATFORM_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im04MDAwNS10ZXN0LWtpZCJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6ImFkbWluIiwicGxhdGZvcm1fYWRtaW4iOnRydWV9fQ.x6um6bT-VysmVR12tT-NbGQl5m8Q1tQbT0J0tcm2UNOWmJ4-nyIu0q-LYniDxFC8LwovQYdqo4R24PcaBT3JTEtD3Msg9-PlB6C1_hgLiEpFg6oqYqKdy3qW8-p6c8NTguqKWWB8LNXOnoXZTsW6FCBDs3Lb0ucc6wpEXFiT44nPkRyC2uCDEjPwG3iEkBGRA9sZ4s_hMAqLdZLN_kH9LSELoGsZFZZlxiyXCyAnX1UtmhuyGLNo4jwsvx99SU8cKzICQljopjfoxWMcvkZ3bzU8aphsgX1emPwGKRkY-6M1hzec-P2BNcye3jOpPoo8v-WlVsL4LHengyyPzFeYkg";
// role=admin, NO platform_admin → tenant admin, must be 403.
const TENANT_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im04MDAwNS10ZXN0LWtpZCJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6ImFkbWluIn19.xOayzWVqaX6io92p8Q34Oiku4pepbpa1uaRG80na9fg6-dD3NNRGM3BKl6ez-ps6m2WucgWV_MT7NEnolgrMyP7EZgVMSZJzHCV9Rp2Iz0wFn52tHGSEXrNqvW3_Vk0U9dl9CwG_m1HVLuKj8rF6KHCsJTuW5q0uCWVKp_b8ore0N6O6lwaZQRwrGBx8bpgrdvdwIkHNgrn_Fz5d8acrxTliRPLN-jNWQO2jiUGAeCY5EFkcv2-ZE-DmVCqDJoGsbfNk_GLvykdIE2bG12BUGO3j5dDX_jAbpEaJMcKJaNAZjvXU8d0yHjqRwQ96wM9a-336yXE-Q_zfXNiu7qAuPA";

// ── Verifier proof (no DB) ───────────────────────────────────────────────────
// Drives the real oidc.Verifier so the fixture is proven against production
// signature + claim-extraction code, independent of the register handler / DB.

fn freePrincipal(p: oidc.Principal) void {
    ALLOC.free(p.subject);
    ALLOC.free(p.issuer);
    if (p.tenant_id) |v| ALLOC.free(v);
    if (p.org_id) |v| ALLOC.free(v);
    if (p.workspace_id) |v| ALLOC.free(v);
    if (p.role) |v| ALLOC.free(v);
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

test "fixture tokens verify through the real oidc verifier; platform_admin parses fail-closed" {
    const admin = try verify(PLATFORM_ADMIN_TOKEN);
    defer freePrincipal(admin);
    try std.testing.expect(admin.platform_admin);
    try std.testing.expectEqualStrings("admin", admin.role.?);

    const tenant = try verify(TENANT_ADMIN_TOKEN);
    defer freePrincipal(tenant);
    try std.testing.expect(!tenant.platform_admin); // absent claim ⇒ false
    try std.testing.expectEqualStrings("admin", tenant.role.?);
}

// ── Register-handler authz (DB-backed) ───────────────────────────────────────

// SAFETY: populated by configureRegistry (with the harness pool) before the
// middleware chain — and thus the lookup — ever reads it.
var api_key_ctx: api_key_lookup.Ctx = undefined;
// SAFETY: populated by configureRegistry (with the harness pool) before the
// runner-bearer middleware — and thus the lookup — ever reads it. Wired so a
// minted `zrn_` resolves against `fleet.runners` (the harness default uses a
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
    const key_hash = api_key.sha256Hex(ZMB_T_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys (id, tenant_id, key_name, key_hash, created_by, active)
        \\VALUES ($1::uuid, $2::uuid, 'runner-enroll-test-key', $3, 'user_enroll_test', TRUE)
        \\ON CONFLICT (key_hash) DO NOTHING
    , .{ API_KEY_ROW_ID, TENANT_ID, key_hash[0..] });
}

fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.api_keys WHERE id = $1::uuid", .{API_KEY_ROW_ID}) catch |err|
        std.log.warn("cleanup api_keys ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id = 'host-enroll-test'", .{}) catch |err|
        std.log.warn("cleanup runners ignored: {s}", .{@errorName(err)});
}

test "register: a platform_admin JWT mints a zrn_ (201)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
    try std.testing.expect(resp.bodyContains("zrn_"));
}

test "register: a tenant-admin JWT without platform_admin is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(TENANT_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
}

test "register: a zmb_t_ api_key cannot enroll a runner (403)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(ZMB_T_KEY)).json(REGISTER_BODY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
}

test "register: the mint records last_seen_at = 0 (never connected → registered)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const mint = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
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

// ── Operator-plane fleet read (GET /v1/fleet/runners) ────────────────────────
// Same platform-admin gate as enrollment; read-only; derives liveness and never
// leaks the token hash or the raw zrn_.

test "fleet list: a platform_admin JWT lists the fleet with derived liveness (200)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const mint = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer mint.deinit();
    try mint.expectStatus(.created);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("host-enroll-test"));
    try std.testing.expect(resp.bodyContains("registered")); // never-connected liveness
    try std.testing.expect(!resp.bodyContains("token_hash")); // invariant: hash never leaves
    try std.testing.expect(!resp.bodyContains("zrn_")); // the raw token is mint-only
}

test "fleet list: a tenant-admin JWT is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(TENANT_ADMIN_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
}

test "fleet list: a zmb_t_ api_key is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(ZMB_T_KEY)).send();
    defer resp.deinit();
    try resp.expectStatus(.forbidden);
    try resp.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
}

// Enrollment is mint-by-API only: the `zombie-runner register` CLI was retired,
// so there is no binary-spawned register arm. The handler authz above is the
// enrollment contract; the `zrn_` is minted server-side from the dashboard's
// session-authed POST (proven here directly against the live HTTP surface).
