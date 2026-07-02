// Integration tests — the generic connector platform:
//
//  * unknown provider → 404 whose body names it (end-to-end via the
//    Bearer-less callback route; the connect/status routes reach the same
//    `registry.respondUnknown` line, pinned here by pure router-match tests)
//  * unconfigured provider → 503 UZ-CONN-001, fail-loud, no partial state
//    (a registry provider whose `<provider>-app` vault bag is absent)
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const router = @import("../../router.zig");
const test_fixtures = @import("../../../db/test_fixtures.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const oauth2 = @import("oauth2.zig");
const slack_spec = @import("slack/spec.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

// UUIDv7-shaped fixtures, distinct from other suites (parallel-runner safe).
const TENANT_ID = "0195c108-0000-7000-8000-f00000000001";
const TENANT_NAME = "m108-registry-suite";
const ADMIN_WS = "0195c108-0001-7000-8000-000000000001";
const TARGET_WS = "0195c108-0002-7000-8000-000000000002";
const SIGNING_SECRET = "m108-registry-signing-secret-key";
const UNKNOWN_PROVIDER = "nope";

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

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
    preClean(alloc, conn);

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
    const key = try credential_key.allocKeyName(alloc, common.PROVIDER_SLACK);
    defer alloc.free(key);
    if (vault.loadJson(alloc, conn, TARGET_WS, key)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {} // any load failure = no readable handle = no partial state
}

fn preClean(alloc: std.mem.Allocator, conn: *pg.Conn) void {
    const key = credential_key.allocKeyName(alloc, common.PROVIDER_SLACK) catch return;
    defer alloc.free(key);
    _ = vault.deleteCredential(conn, TARGET_WS, key) catch |e| std.log.warn("preclean vault ignored: {s}", .{@errorName(e)});
}
