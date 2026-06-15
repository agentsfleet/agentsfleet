// Fleet operator-plane mutation over the live HTTP surface:
// PATCH /v1/fleet/runners/{id} is platform-admin-only, idempotent, and updates
// the admin_state that runnerBearer enforces on the self-plane.

const std = @import("std");
const clock = @import("common").clock;
const auth_mw = @import("../auth/middleware/mod.zig");
const api_key = @import("../auth/api_key.zig");
const api_key_lookup = @import("../cmd/api_key_lookup.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const error_registry = @import("../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("handlers/agents/test_sse_client.zig");
const sse_fixtures = @import("handlers/agents/sse_test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TEST_ISSUER = "https://clerk.test.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f02";
const API_KEY_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7003";
const OP_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7004";
const RUNNER_TOKEN_BODY_HEX_CHARS: usize = 60;
const TENANT_KEY_BODY_CHARS: usize = 48;
const OP_RAW_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "p" ** RUNNER_TOKEN_BODY_HEX_CHARS;
const AGT_T_KEY = auth_mw.tenant_api_key.TENANT_KEY_PREFIX ++ "d" ** TENANT_KEY_BODY_CHARS;

const BODY_CORDON = "{\"action\":\"cordon\"}";
const BODY_DRAIN = "{\"action\":\"drain\"}";
const BODY_REVOKE = "{\"action\":\"revoke\"}";
const BODY_BAD_ACTION = "{\"action\":\"pause\"}";
const ONE_EVENT: i64 = 1;

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"qXJuc_Hncnu-ZAFKPEhb6qeXXSp1GcUidOyyiyFFwi5bmql2NZH4Quv23LhHsAKM8L5950bvTQppdzcJ8zWQKx9F8kViZgaG1Ghagoz2a2BMjeSHLFu_gfsxP6y752WUcZ1uHUGnWm9WsDE7xMfbOOpcUoOc_RxiRhwuXjR3zw6J8Vl4DABKQXq_jb6l5nyDWOsi9FopsaS6FKpQoiWO4DWHEHVVNA7RxoYtb1ew9u4qSq4dyeyb6sOXBWuc9wOjSXcuEm30qYsvZJ8ORSh1hxdDaArUCXQKp_DPVJBO7Mmu_EAnOcSsFeZ-kgLVD7yJp_Yq983-s9odwX0TxlL8Lw","e":"AQAB","kid":"m80005-test-kid","use":"sig","alg":"RS256"}]}
;
const PLATFORM_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im04MDAwNS10ZXN0LWtpZCJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiIsInBsYXRmb3JtX2FkbWluIjp0cnVlfX0.H3gZWcqBWYnREFPQAbnoIzhV33ckaYyo37clfhGekxy4TMM96QuUbeyHJW0CnuMRS6UueCjwiidW3mfkINdfQy6-Y4aERoqPvfYQ7QGiwMSPU63heJKxS4fzHzbdDMfO1XoAEcj333xJ8NyvkdBXEbKS9k0LA2-4mczKXLnWkEHnAfWslsK1hdLdIf4rNYP4KahrV25QU-8RirkUTV5jUUgH3HuPMTF976FZX_Q6pL2vW6i1iS2S4iMVwmBdBPlMCPLfjc3Yi9EIP0eBCkWCwZrp1nD5U74Akb6Yh4LCJw9xbhj4kI4jr9e-zwOh7FH_fzbxgJUxHg2jl9pLSAofGw";
const TENANT_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im04MDAwNS10ZXN0LWtpZCJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiJ9fQ.hwmrKrb3wFrLg6Bni7UJupLBC77ZVz9lLgCzTCLPrbSqfj25y-VzQUgA7aiXWJtmPlH565zIU2FCmOwD2oxDDlPSA2XJB0GkHQQT0_jWLBIK6il72YAhijRheJJKiRa2K7c1UABp9CPC2PPd8cEAPy2e5-N884T4y_jQo6qhn-bM2lHJ3i3SOG-vVHkt35uA-_Kgsg5DZHrCwsbWXc1jRM8_wirbtFIWzasEYMfjyt3HO15mMhiBUlo6-v28z_NQkA1WZ3BTFtUvpEbH5ZLhNNEQndbx3nqmF6U1F1YvgvR1krtwCJGFXXiv5RUuDR5fqMnH6DytrSxd7EpAvAlqnQ";

// SAFETY: populated by configureRegistry before the middleware chain reads it.
var api_key_ctx: api_key_lookup.Ctx = undefined;
// SAFETY: populated by configureRegistry before runnerBearer reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    api_key_ctx = .{ .pool = h.pool };
    reg.tenant_api_key_mw = .{ .host = &api_key_ctx, .lookup = api_key_lookup.lookup };
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
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
        \\VALUES ($1::uuid, 'Fleet Operator Test Tenant', $2::bigint, $2::bigint)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TENANT_ID, now_ms });
    const key_hash = api_key.sha256Hex(AGT_T_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys (uid, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'fleet-operator-test-key', '', $3::text, 'user_fleet_operator_test', TRUE, $4::bigint, $4::bigint)
        \\ON CONFLICT (key_hash) DO NOTHING
    , .{ API_KEY_ROW_ID, TENANT_ID, key_hash[0..], now_ms });
}

fn seedRunner(h: *TestHarness, admin_state: protocol.AdminState) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const hash = api_key.sha256Hex(OP_RAW_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'host-operator-test', $2::text, 'dev_none', $3::text, '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO UPDATE SET admin_state = EXCLUDED.admin_state, updated_at = 0
    , .{ OP_RUNNER_ID, hash[0..], @tagName(admin_state) });
}

fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{OP_RUNNER_ID}) catch |err|
        std.log.warn("cleanup fleet runner ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.api_keys WHERE uid = $1::uuid", .{API_KEY_ROW_ID}) catch |err|
        std.log.warn("cleanup api key ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{TENANT_ID}) catch |err|
        std.log.warn("cleanup tenant ignored: {s}", .{@errorName(err)});
}

fn patchRunner(h: *TestHarness, bearer: []const u8, body: []const u8) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ protocol.PATH_FLEET_RUNNERS, OP_RUNNER_ID });
    defer ALLOC.free(path);
    return (try (try h.request(.PATCH, path).bearer(bearer)).json(body)).send();
}

fn getRunnerState(h: *TestHarness) !protocol.AdminState {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT admin_state FROM fleet.runners WHERE id = $1::uuid", .{OP_RUNNER_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const raw = try row.get([]u8, 0);
    return std.meta.stringToEnum(protocol.AdminState, raw) orelse error.TestUnexpectedResult;
}

fn getRunnerUpdatedAt(h: *TestHarness) !i64 {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT updated_at FROM fleet.runners WHERE id = $1::uuid", .{OP_RUNNER_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn eventCount(h: *TestHarness, event_type: protocol.RunnerEventType) !i64 {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE runner_id = $1::uuid AND event_type = $2
    , .{ OP_RUNNER_ID, @tagName(event_type) }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

test "fleet runner PATCH cordons idempotently then drains" {
    const h = try startHarness();
    defer h.deinit();
    try seedTenantAndApiKey(h);
    try seedRunner(h, .active);
    defer cleanup(h);

    const cordon = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_CORDON);
    defer cordon.deinit();
    try cordon.expectStatus(.ok);
    try std.testing.expect(cordon.bodyContains("\"admin_state\":\"cordoned\""));
    try std.testing.expectEqual(protocol.AdminState.cordoned, try getRunnerState(h));
    const cordoned_at = try getRunnerUpdatedAt(h);

    const again = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_CORDON);
    defer again.deinit();
    try again.expectStatus(.ok);
    try std.testing.expectEqual(cordoned_at, try getRunnerUpdatedAt(h));
    try std.testing.expectEqual(ONE_EVENT, try eventCount(h, .runner_cordoned));

    const drain = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_DRAIN);
    defer drain.deinit();
    try drain.expectStatus(.ok);
    try std.testing.expectEqual(protocol.AdminState.draining, try getRunnerState(h));
    try std.testing.expectEqual(ONE_EVENT, try eventCount(h, .runner_draining));
}

test "fleet runner PATCH is platform-admin gated" {
    const h = try startHarness();
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const tenant = try patchRunner(h, TENANT_ADMIN_TOKEN, BODY_CORDON);
    defer tenant.deinit();
    try tenant.expectStatus(.forbidden);
    try tenant.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);

    const api_key_resp = try patchRunner(h, AGT_T_KEY, BODY_CORDON);
    defer api_key_resp.deinit();
    try api_key_resp.expectStatus(.forbidden);
    try api_key_resp.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
}

test "fleet runner PATCH revoke makes the next runner-plane call unauthorized" {
    const h = try startHarness();
    defer h.deinit();
    try seedTenantAndApiKey(h);
    try seedRunner(h, .active);
    defer cleanup(h);

    const revoke = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_REVOKE);
    defer revoke.deinit();
    try revoke.expectStatus(.ok);
    try std.testing.expectEqual(protocol.AdminState.revoked, try getRunnerState(h));

    const denied = try (try h.get(protocol.PATH_RUNNER_SELF).bearer(OP_RAW_TOKEN)).send();
    defer denied.deinit();
    try denied.expectStatus(.unauthorized);
    try denied.expectErrorCode(error_registry.ERR_RUN_ADMIN_STATE_BLOCKED);
}

test "fleet runner PATCH rejects malformed actions and missing runners" {
    const h = try startHarness();
    defer h.deinit();
    try seedTenantAndApiKey(h);
    defer cleanup(h);

    const bad = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_BAD_ACTION);
    defer bad.deinit();
    try bad.expectStatus(.bad_request);
    try bad.expectErrorCode(error_registry.ERR_INVALID_REQUEST);

    const missing = try patchRunner(h, PLATFORM_ADMIN_TOKEN, BODY_CORDON);
    defer missing.deinit();
    try missing.expectStatus(.not_found);
    try missing.expectErrorCode(error_registry.ERR_RUNNER_NOT_FOUND);
}

// ── Fleet streams listing (StreamRegistry operator surface) ─────────────────

const STREAMS_PATH = "/v1/fleet/streams";
const AGENTSFLEET_FLEET_STREAM = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";

test "fleet streams: non-GET methods are 405" {
    // router.match resolves /v1/fleet/streams for ANY method; the invoke fn
    // is the only 405 gate — without this pin a POST would fall through to
    // the listing handler.
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const denied = try (try (try h.request(.POST, STREAMS_PATH).bearer(PLATFORM_ADMIN_TOKEN)).json("{}")).send();
    defer denied.deinit();
    try denied.expectStatus(.method_not_allowed);
}

test "fleet streams: platform-admin lists live streams; tenant admin is 403" {
    sse_fixtures.requireRedisEnvOrSkip() catch return error.SkipZigTest;
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try sse_fixtures.seedWorkspace(conn);
        try sse_fixtures.seedAgent(conn, AGENTSFLEET_FLEET_STREAM, "fleet-streams");
    }

    // tenant admin (verified JWT, but no platform_admin claim) → 403
    const denied = try (try h.get(STREAMS_PATH).bearer(TENANT_ADMIN_TOKEN)).send();
    defer denied.deinit();
    try denied.expectStatus(.forbidden);

    // platform admin, no live streams → empty listing
    const empty = try (try h.get(STREAMS_PATH).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer empty.deinit();
    try empty.expectStatus(.ok);
    try std.testing.expect(empty.bodyContains("\"total\":0"));

    // a live stream appears with its workspace + agent (the platform-admin
    // token's workspace metadata matches the seeded fixture workspace)
    const stream_path = try sse_fixtures.streamPath(ALLOC, AGENTSFLEET_FLEET_STREAM);
    defer ALLOC.free(stream_path);
    var sc = try SseClient.connect(ALLOC, h.port, stream_path, .{ .bearer = PLATFORM_ADMIN_TOKEN });
    @import("common").sleepNanos(sse_fixtures.SUBSCRIBE_SETTLE_NS);

    const listed = try (try h.get(STREAMS_PATH).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer listed.deinit();
    try listed.expectStatus(.ok);
    try std.testing.expect(listed.bodyContains(AGENTSFLEET_FLEET_STREAM));
    try std.testing.expect(listed.bodyContains(sse_fixtures.TEST_WORKSPACE_ID));
    try std.testing.expect(listed.bodyContains("\"total\":1"));

    // teardown via the drain (no publisher client in this suite): the
    // stream socket is shut, the thread deregisters, drain returns settled
    h.streams.drain();
    sc.closeStream();
    sc.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    sse_fixtures.cleanupWorkspaceData(conn);
}
