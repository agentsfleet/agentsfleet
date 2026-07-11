// Integration tests — the generic connector platform:
//
//  * unknown provider → 404 whose body names it (end-to-end via the
//    Bearer-less callback route; the connect/status routes reach the same
//    `registry.respondUnknown` line, pinned here by pure router-match tests)
//  * unconfigured provider → 503 UZ-CONN-001, fail-loud, no partial state
//    (a registry provider whose `<provider>-app` vault bag is absent) — the
//    callback side AND the connect side (Dim 1.1's literal wording)
//  * the Bearer-authed generic connect/status flows (never integration-tested
//    before this suite — M102/M106 shipped connect + status without one):
//    scope enforcement, oauth2 authorize-URL minting, app_install URL
//    building, status not_connected → connected flips for both archetypes
//  * exchange failure classes end-to-end: missing state/code 400s, vendor
//    unreachable → 502 UZ-CONN-003 (through bounded_fetch's pin refusal),
//    vendor 5xx → 502 exchange-failed (loopback fake answering 500)
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const router = @import("../../router.zig");
const test_port = @import("../../test_port.zig");
const scope_tokens = @import("../../test_scope_tokens.zig");
const test_fixtures = @import("../../../db/test_fixtures.zig");
const vault = @import("../../../state/vault.zig");
const oauth2 = @import("oauth2.zig");
const connector_state = @import("state.zig");
const slack_spec = @import("slack/spec.zig");
const github_spec = @import("github/spec.zig");

const TestHarness = harness_mod.TestHarness;
const net = std.Io.net;
const testing = std.testing;

// UUIDv7-shaped fixtures, distinct from other suites (parallel-runner safe).
const TENANT_ID = "0195c108-0000-7000-8000-f00000000001";
const TENANT_NAME = "m108-registry-suite";
const ADMIN_WS = "0195c108-0001-7000-8000-000000000001";
const TARGET_WS = "0195c108-0002-7000-8000-000000000002";
const SIGNING_SECRET = "m108-registry-signing-secret-key";
const UNKNOWN_PROVIDER = "nope";

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn mintLatestGithubState(alloc: std.mem.Allocator, h: *TestHarness) ![]const u8 {
    const state = try connector_state.mint(alloc, &h.queue, github_spec.STATE, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    errdefer alloc.free(state);
    try connector_state.markLatest(&h.queue, github_spec.STATE, TARGET_WS, state);
    return state;
}

// ── Pure router-shape tests (no DB) — the generic trio resolves any
// {provider} segment; provider resolution is the handler layer's job. ────────

test "router: the generic connector trio captures workspace + provider" {
    const connect = router.match("/v1/workspaces/ws-1/connectors/slack/connect", .POST) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings("ws-1", connect.connector_connect.workspace_id);
    try testing.expectEqualStrings("slack", connect.connector_connect.provider);

    const status = router.match("/v1/workspaces/ws-1/connectors/github", .GET) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings("github", status.connector_status.provider);

    const callback = router.match("/v1/connectors/slack/callback", .GET) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings("slack", callback.connector_callback);

    // The catalog is the workspace-nested collection whose items are the status
    // routes; its capture is the workspace id (workspace_id is a PATH param, per
    // the universal standard — never a query).
    const catalog = router.match("/v1/workspaces/ws-1/connectors", .GET) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings("ws-1", catalog.connector_catalog);

    // An unknown id still ROUTES (the registry 404s it with a naming body —
    // proven end-to-end below); the events ingress stays bespoke.
    const unknown = router.match("/v1/workspaces/ws-1/connectors/nope/connect", .POST) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings(UNKNOWN_PROVIDER, unknown.connector_connect.provider);
    const events = router.match("/v1/connectors/slack/events", .POST) orelse return error.TestUnexpectedResult;
    try testing.expect(events == .slack_events);
}

// ── End-to-end: unknown provider → 404 naming it (Dim 3.1) ──────────────────

test "integration: unknown provider callback is a 404 whose body names it" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get("/v1/connectors/" ++ UNKNOWN_PROVIDER ++ "/callback?state=whatever").send();
    defer r.deinit();
    try r.expectStatus(.not_found);
    try testing.expect(r.bodyContains(UNKNOWN_PROVIDER));
    try testing.expect(r.bodyContains("UZ-CONN-004"));
}

// ── End-to-end: registry provider without its `<provider>-app` bag →
// 503 UZ-CONN-001, no partial state (Dim 1.1) ────────────────────────────────

test "integration: unconfigured provider fails loud 503, no partial state" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    preClean(conn);

    // Signing secret present, admin workspace wired — but NO slack-app vault
    // bag: the platform app is unprovisioned.
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const state = try oauth2.mintState(alloc, &h.queue, slack_spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?code=fake-code&state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.service_unavailable);
    try testing.expect(r.bodyContains("UZ-CONN-001"));

    // Fail-loud with NO partial state: no vault handle was written.
    if (vault.loadJson(alloc, conn, TARGET_WS, common.PROVIDER_SLACK)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {} // any load failure = no readable handle = no partial state
}

fn preClean(conn: *pg.Conn) void {
    _ = vault.deleteCredential(conn, TARGET_WS, common.PROVIDER_SLACK) catch |e| std.log.warn("preclean vault ignored: {s}", .{@errorName(e)});
    // Belt-and-suspenders: the "unconfigured" assertion depends on ADMIN_WS
    // having NO slack-app bag. A sibling seed-creds test that crashed before
    // its cleanup could leave one — and then this test would load real creds
    // and dial the real slack.com token endpoint. Delete it unconditionally.
    _ = vault.deleteCredential(conn, ADMIN_WS, "slack-app") catch |e| std.log.warn("preclean slack-app ignored: {s}", .{@errorName(e)});
}

// ── Bearer-authed generic flows ──────────────────────────────────────────────
// The persona tokens (test_scope_tokens) bind this fixed tenant/workspace pair;
// mirrored from rbac_http_integration_test (pin: literals are the fixture).
const AUTHED_TENANT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const AUTHED_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const GITHUB_TEST_SLUG = "m108-test-app";

fn startAuthedHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = noopRegistry,
        .inline_jwks_json = scope_tokens.JWKS,
        .issuer = scope_tokens.ISSUER,
        .audience = scope_tokens.AUDIENCE,
    });
}

/// Tenant + workspace rows for the persona-bound ids, plus this suite's admin
/// workspace (the platform-app vault home). ON CONFLICT keeps parallel suites
/// collision-safe.
fn seedAuthedFixtures(conn: *pg.Conn) !void {
    const now_ms = common.clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO tenants (tenant_id, name, created_at, updated_at) VALUES ($1, 'M108 Registry Authed Tenant', $2, $2) ON CONFLICT (tenant_id) DO NOTHING",
        .{ AUTHED_TENANT, now_ms },
    );
    _ = try conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_at) VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING",
        .{ AUTHED_WS, AUTHED_TENANT, now_ms },
    );
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
}

fn seedSlackAppCreds(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "client_id", .{ .string = "m108-test-client-id" });
    try obj.put(alloc, "client_secret", .{ .string = "m108-test-client-secret" });
    try test_fixtures.storeVaultJson(alloc, conn, ADMIN_WS, "slack-app", .{ .object = obj });
}

fn deleteVaultKey(conn: *pg.Conn, ws: []const u8, key: []const u8) void {
    _ = vault.deleteCredential(conn, ws, key) catch |e| std.log.warn("vault cleanup ignored: {s}", .{@errorName(e)});
}

fn deleteFleetHandle(conn: *pg.Conn, ws: []const u8, provider: []const u8) void {
    deleteVaultKey(conn, ws, provider);
}

test "integration: connect without connector:write is a 403 on the generic route" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAuthedFixtures(conn);

    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/slack/connect").json("{}")).bearer(scope_tokens.VIEWER)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
}

test "integration: connect for a provider without its platform app bag is a loud 503 (Dim 1.1, connect side)" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteVaultKey(conn, ADMIN_WS, "slack-app"); // ensure the bag is absent
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/slack/connect").json("{}")).bearer(scope_tokens.TENANT_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.service_unavailable);
    try r.expectErrorCode("UZ-CONN-001");
    try testing.expect(r.bodyContains("Slack")); // display-name wording preserved
}

test "integration: slack connect returns the provider authorize URL with a bound state" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    try seedSlackAppCreds(alloc, conn);
    defer deleteVaultKey(conn, ADMIN_WS, "slack-app"); // cleans up even if an assert below fails
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/slack/connect").json("{}")).bearer(scope_tokens.TENANT_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try testing.expect(r.bodyContains("install_url"));
    try testing.expect(r.bodyContains("slack.com/oauth/v2/authorize")); // the registry entry's endpoint
    try testing.expect(r.bodyContains("state=")); // the signed single-use state rides the URL
    try testing.expect(r.bodyContains("m108-test-client-id"));
}

test "integration: github connect builds the App install URL from the configured slug" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAuthedFixtures(conn);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.github_app_slug = GITHUB_TEST_SLUG;

    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/github/connect").json("{}")).bearer(scope_tokens.TENANT_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try testing.expect(r.bodyContains("github.com/apps/" ++ GITHUB_TEST_SLUG ++ "/installations/new?state="));
}

test "integration: github connect without an App slug is a loud 503" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAuthedFixtures(conn);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.github_app_slug = null; // platform config absent → degrade closed

    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/github/connect").json("{}")).bearer(scope_tokens.TENANT_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.service_unavailable);
    try r.expectErrorCode("UZ-CONN-001");
}

// ── Callback request-shape failures (Bearer-less route) ─────────────────────

test "integration: callback with a missing state is a 400 invalid request" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    h.ctx.approval_signing_secret = SIGNING_SECRET;

    const r = try h.get("/v1/connectors/slack/callback?code=whatever").send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");
}

test "integration: oauth2 callback with a missing code is a 400 invalid request" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    h.ctx.approval_signing_secret = SIGNING_SECRET;

    const state = try oauth2.mintState(alloc, &h.queue, slack_spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");
}

// ── Status flips (both archetypes) ───────────────────────────────────────────

test "integration: slack status flips not_connected → connected and surfaces the team name" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_SLACK);

    const path = "/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/slack";
    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try testing.expect(r.bodyContains("not_connected"));
    }

    var handle: std.json.ObjectMap = .empty;
    defer handle.deinit(alloc);
    try handle.put(alloc, "bot_token", .{ .string = "xoxb-m108-status-tok" });
    try handle.put(alloc, "team_name", .{ .string = "Acme M108" });
    try test_fixtures.storeVaultJson(alloc, conn, AUTHED_WS, common.PROVIDER_SLACK, .{ .object = handle });

    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try testing.expect(r.bodyContains("\"status\":\"connected\""));
        try testing.expect(r.bodyContains("Acme M108"));
    }

    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_SLACK);
}

test "integration: catalog reflects the registry with correct configured/connected flags (Dimension 4.1)" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    // The catalog's "configured" lookup targets the platform-admin workspace; wire
    // it (every sibling test does) so the seed/delete of `slack-app` at ADMIN_WS is
    // what that lookup reads. Without it the ctx default "" hits an invalid-UUID cast.
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    // Clean slate: no slack platform bag, no slack handle for this workspace.
    _ = vault.deleteCredential(conn, ADMIN_WS, "slack-app") catch {};
    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_SLACK);

    const path = "/v1/workspaces/" ++ AUTHED_WS ++ "/connectors";

    // Baseline — registry-driven (every provider present), slack unconfigured +
    // not connected.
    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        // No hard-coded list — every registry provider appears.
        inline for (.{ "slack", "github", "zoho", "jira", "linear" }) |id| {
            try testing.expect(r.bodyContains("\"id\":\"" ++ id ++ "\""));
        }
        // Field order is CatalogEntry declaration order (compact JSON).
        try testing.expect(r.bodyContains("\"id\":\"slack\",\"archetype\":\"oauth2\",\"display_name\":\"Slack\",\"configured\":false,\"connected\":false"));
    }

    // Provision slack's platform bag + connect this workspace.
    try seedSlackAppCreds(alloc, conn);
    var handle: std.json.ObjectMap = .empty;
    defer handle.deinit(alloc);
    try handle.put(alloc, "bot_token", .{ .string = "xoxb-m108-catalog-tok" });
    try test_fixtures.storeVaultJson(alloc, conn, AUTHED_WS, common.PROVIDER_SLACK, .{ .object = handle });

    // slack now flips to configured (platform bag) AND connected (handle).
    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try testing.expect(r.bodyContains("\"id\":\"slack\",\"archetype\":\"oauth2\",\"display_name\":\"Slack\",\"configured\":true,\"connected\":true"));
    }

    // Foreign workspace / no scope is already covered by the IDOR + scope tests;
    // clean up this suite's shared platform bag + handle.
    _ = vault.deleteCredential(conn, ADMIN_WS, "slack-app") catch {};
    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_SLACK);
}

// An unconfigured deployment leaves the platform-admin workspace unset. The
// catalog must degrade to configured:false, never 500 on the invalid-UUID cast.
test "integration: catalog degrades to configured:false when the platform-admin workspace is unset (no 500)" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    // Deliberately DO NOT set h.ctx.platform_admin_workspace_id — it stays "" (the
    // unconfigured-deployment default). The configured lookup must be skipped.

    const path = "/v1/workspaces/" ++ AUTHED_WS ++ "/connectors";
    const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.ok); // not 500
    // Every oauth2/app_install provider reads not-configured (no admin bag to check).
    try testing.expect(r.bodyContains("\"id\":\"slack\",\"archetype\":\"oauth2\",\"display_name\":\"Slack\",\"configured\":false"));
}

test "integration: github status reads the installation handle" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_GITHUB);

    const path = "/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/github";
    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try testing.expect(r.bodyContains("not_connected"));
    }

    var handle: std.json.ObjectMap = .empty;
    defer handle.deinit(alloc);
    try handle.put(alloc, "integration", .{ .string = "github" });
    try handle.put(alloc, "installation_id", .{ .string = "1234567" });
    try test_fixtures.storeVaultJson(alloc, conn, AUTHED_WS, common.PROVIDER_GITHUB, .{ .object = handle });

    {
        const r = try (try h.get(path).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try testing.expect(r.bodyContains("\"status\":\"connected\""));
    }

    deleteFleetHandle(conn, AUTHED_WS, common.PROVIDER_GITHUB);
}

// ── app_install callback e2e (the archetype the oauth2 suites don't cover) ──

test "integration: github callback requires a user-authorization code" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    deleteFleetHandle(conn, TARGET_WS, common.PROVIDER_GITHUB);
    h.ctx.approval_signing_secret = SIGNING_SECRET;

    // app_install state is minted against github's OWN domain binding.
    const state = try mintLatestGithubState(alloc, h);
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/github/callback?installation_id=42424242&state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");

    deleteFleetHandle(conn, TARGET_WS, common.PROVIDER_GITHUB);
}

test "integration: github callback with a non-numeric installation_id is a 400, no handle written" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    deleteFleetHandle(conn, TARGET_WS, common.PROVIDER_GITHUB);
    h.ctx.approval_signing_secret = SIGNING_SECRET;

    const state = try mintLatestGithubState(alloc, h);
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/github/callback?installation_id=not-a-number&state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");

    if (vault.loadJson(alloc, conn, TARGET_WS, common.PROVIDER_GITHUB)) |p| {
        var pp = p;
        pp.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {}
}

// ── Cross-workspace authorization (the generic trio's authorizeWorkspace gate)

test "integration: a valid token cannot connect/read a workspace it doesn't own (IDOR)" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    // A second tenant's workspace the TENANT_ADMIN persona (bound to AUTHED_WS)
    // has no claim on — same suite tenant, distinct workspace id.
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    // connect on a foreign workspace → 403 (authorizeWorkspace denies).
    {
        const r = try (try (try h.post("/v1/workspaces/" ++ TARGET_WS ++ "/connectors/slack/connect").json("{}")).bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    // status on a foreign workspace → 403 likewise.
    {
        const r = try (try h.get("/v1/workspaces/" ++ TARGET_WS ++ "/connectors/slack").bearer(scope_tokens.TENANT_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
}

// ── Vendor-failure classes on the exchange (e2e through bounded_fetch) ──────

test "integration: an unreachable vendor is a 502 UZ-CONN-003 (pin refused, never unbounded)" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    try seedSlackAppCreds(alloc, conn);
    defer deleteVaultKey(conn, ADMIN_WS, "slack-app"); // cleans up even if an assert below fails
    defer deleteFleetHandle(conn, TARGET_WS, common.PROVIDER_SLACK);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    // Port 1 (tcpmux) never listens on a dev/CI host: the dial is refused,
    // bounded_fetch refuses the call (VendorUnreachable), and the callback
    // maps it to the vendor-call failure code — no vault write.
    h.ctx.connector_oauth_token_endpoint_override = "http://127.0.0.1:1/api/oauth.v2.access";

    const state = try oauth2.mintState(alloc, &h.queue, slack_spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?code=fake-code&state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.bad_gateway);
    try r.expectErrorCode("UZ-CONN-003");
}

/// A vendor that answers every request 500 — the exchange-failed shape.
/// Mirrors the M106 suite's FakeSlack accept/respond scaffolding.
const FakeVendor500 = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *FakeVendor500) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *FakeVendor500) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn acceptLoop(self: *FakeVendor500) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            handleConn(stream);
        }
    }

    fn handleConn(stream: net.Stream) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var sreader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var swriter = stream.writer(io, &write_buf);
        var http_server = std.http.Server.init(&sreader.interface, &swriter.interface);
        var req = http_server.receiveHead() catch return;
        req.respond("{\"ok\":false}", .{
            .status = .internal_server_error,
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = "application/json" }},
        }) catch return;
    }
};

test "integration: a vendor 5xx on the exchange is a 502 exchange-failed" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    try seedSlackAppCreds(alloc, conn);
    defer deleteVaultKey(conn, ADMIN_WS, "slack-app"); // cleans up even if an assert below fails

    var fake: FakeVendor500 = undefined;
    try fake.start();
    defer fake.shutdown();

    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    const override = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/api/oauth.v2.access", .{fake.port});
    defer alloc.free(override);
    h.ctx.connector_oauth_token_endpoint_override = override;

    const state = try oauth2.mintState(alloc, &h.queue, slack_spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?code=fake-code&state={s}", .{state});
    defer alloc.free(path);

    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.bad_gateway);
    try r.expectErrorCode("UZ-SLK-022");
    // Exchange failed → no vault write happened (the exchange precedes it).
    if (vault.loadJson(alloc, conn, TARGET_WS, common.PROVIDER_SLACK)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {}
}
