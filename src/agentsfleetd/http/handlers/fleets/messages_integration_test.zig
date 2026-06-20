// HTTP integration tests for fleet messages endpoint.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. Success-path
// tests additionally require a reachable Redis (XADDs land on
// fleet:{id}:events) and self-skip via `h.tryConnectRedis()`.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aff01";
const FLEET_IDLE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa01";
const AGENTSFLEET_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa02";
const AGENTSFLEET_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa03";
const SESSION_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa10";
const EXECUTION_STARTED_AT_MS: i64 = 1000;

const ACTIVE_EXEC_ID = "test-exec-messages-001";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJvcGVyYXRvciJ9fQ.eEQp3HyUFsV1bRBDvww3DirCY1R-vrASYT3KXnTeXBa8Owuag8Mc1I_v93XBatf-t-Y0qd6r9uNQuRiRpuXkrC01MJwyPnyvKDYHFAX828PIMdFgZ5FUGU0S6r1B4B8FaVZnfMdwyyQW9tCeFBvvh2hkuodoOlkcaJnR98kMrYjGHVoyDQc5H5JnU5O8Kkb9STE-XR-3b8VdOlGJR-ljX4Vw8Fipo5p7fo_VdhhUXD2C974DrbQWtsXhqUTqOFWAEUcUMM2ODH8pEFWhG8poHVP8LLWCcSFxZDN_Ia3dNR8OK9SEblCPIlfimiMtscqxli-9uC00n62UmLuQtGVlXA";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    _ = h.tryConnectRedis();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'MessagesTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, clock.nowMillis() });
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'msg-idle', '---\nname: msg-idle\n---\ntest', '{"name":"msg-idle"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_IDLE, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'msg-active', '---\nname: msg-active\n---\ntest', '{"name":"msg-active"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_ACTIVE, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_sessions (id, fleet_id, context_json, execution_id, execution_started_at, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, '{}', $3, $4, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET execution_id=EXCLUDED.execution_id, execution_started_at=EXCLUDED.execution_started_at
    , .{ SESSION_ACTIVE, AGENTSFLEET_ACTIVE, ACTIVE_EXEC_ID, EXECUTION_STARTED_AT_MS });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'msg-otherws', '---\nname: msg-otherws\n---\ntest', '{"name":"msg-otherws"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleet_sessions WHERE fleet_id IN ($1, $2, $3)", .{ FLEET_IDLE, AGENTSFLEET_ACTIVE, AGENTSFLEET_OTHER_WS }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id IN ($1, $2)", .{ TEST_WORKSPACE_ID, OTHER_WS_ID }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// ── Auth + body validation (no Redis needed) ────────────────────────────────

test "integration: fleet messages — auth and body validation" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url_idle = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, FLEET_IDLE });
    defer ALLOC.free(url_idle);
    // url_other: caller's workspace in URL path, but fleet lives in OTHER_WS — handler 404.
    const url_other = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, AGENTSFLEET_OTHER_WS });
    defer ALLOC.free(url_other);
    const body_valid = "{\"message\":\"redirect to phase 2\"}";
    const body_empty = "{\"message\":\"\"}";
    const body_toolong = "{\"message\":\"" ++ "x" ** 8193 ++ "\"}";

    { // no bearer → 401
        const r = try (try h.post(url_idle).json(body_valid)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    { // fleet in different workspace → 404
        const r = try (try (try h.post(url_other).bearer(TOKEN_OPERATOR)).json(body_valid)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
    }
    { // empty message → 400
        const r = try (try (try h.post(url_idle).bearer(TOKEN_OPERATOR)).json(body_empty)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // missing `message` field entirely → 400 (json parse fails the required field)
        const r = try (try (try h.post(url_idle).bearer(TOKEN_OPERATOR)).json("{}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // message > 8192 bytes → 400
        const r = try (try (try h.post(url_idle).bearer(TOKEN_OPERATOR)).json(body_toolong)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── Idle fleet happy path: 202 + event_id ──────────────────────────────────

test "integration: fleet messages idle — 202 returns event_id from xadd" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, FLEET_IDLE });
    defer ALLOC.free(url);

    const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"proceed to phase 2\"}")).send();
    defer r.deinit();

    try r.expectStatus(.accepted);
    try std.testing.expect(r.bodyContains("\"status\":\"accepted\""));
    try std.testing.expect(r.bodyContains("\"event_id\":\""));

    // The XADD created the stream — drop it so leftover entries don't bleed
    // across runs.
    var del = h.queue.command(&.{ "DEL", "fleet:" ++ FLEET_IDLE ++ ":events" }) catch return;
    defer del.deinit(h.queue.alloc);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── Paused fleet: ingress refuses loudly ) ──────────────────────────────

test "integration: steer paused fleet — 409 UZ-AGT-012; resumed fleet steers fine" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const AGENTSFLEET_PAUSED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa04";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec(
            \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
            \\VALUES ($1, $2, 'msg-paused', '---\nname: msg-paused\n---\ntest', '{"name":"msg-paused"}', 'paused', 0, 0)
            \\ON CONFLICT (id) DO UPDATE SET status = 'paused'
        , .{ AGENTSFLEET_PAUSED, TEST_WORKSPACE_ID });
    }

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, AGENTSFLEET_PAUSED });
    defer ALLOC.free(url);

    { // paused → 409 with the registered code + nothing enqueued
        const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"wake up\"}")).send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains("UZ-AGT-012"));
        // REST §4: every 409 names the state that forbade the transition.
        try std.testing.expect(r.bodyContains("\"current_state\":\"paused\""));
    }

    if (h.has_redis) { // resume → the same steer 202s (terminal refusals never block re-request)
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec("UPDATE core.fleets SET status = 'active' WHERE id = $1", .{AGENTSFLEET_PAUSED});
        const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"wake up\"}")).send();
        defer r.deinit();
        try r.expectStatus(.accepted);
        var del = h.queue.command(&.{ "DEL", "fleet:" ++ AGENTSFLEET_PAUSED ++ ":events" }) catch null;
        if (del) |*d| d.deinit(h.queue.alloc);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1", .{AGENTSFLEET_PAUSED}) catch {};
    cleanupTestData(conn);
}

// ── Active fleet happy path: same single ingress as idle ───────────────────

test "integration: fleet messages active — 202 returns event_id (same single ingress)" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, AGENTSFLEET_ACTIVE });
    defer ALLOC.free(url);

    const r = try (try (try h.post(url).bearer(TOKEN_OPERATOR)).json("{\"message\":\"new objective\"}")).send();
    defer r.deinit();

    try r.expectStatus(.accepted);
    try std.testing.expect(r.bodyContains("\"status\":\"accepted\""));
    try std.testing.expect(r.bodyContains("\"event_id\":\""));

    var del = h.queue.command(&.{ "DEL", "fleet:" ++ AGENTSFLEET_ACTIVE ++ ":events" }) catch return;
    defer del.deinit(h.queue.alloc);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
