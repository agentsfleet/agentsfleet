// HTTP integration tests for M12_001 dashboard endpoints (activity feed,
// agent stop, billing summaries — per-agent and per-workspace).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".
//
// Workspace and tenant IDs are fixed to match the embedded JWT tokens.
// Agent, activity-event, and telemetry IDs are generated per call so
// concurrent or repeated runs never conflict on primary keys. No cleanup
// function is needed: make down && make up resets the DB between runs, and
// unique IDs within a run prevent PK collisions.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Fixed — embedded in TOKEN_USER and TOKEN_OPERATOR.
const TEST_BALANCE_NANOS: i64 = 1000;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJ1c2VyIn19.aSqdpbu-D-1NmzJgcw-7LUJYImlFu-gbrO3fBPlMI6DFvgSGJJg3wAYe5DKJXe5ytCActeAHN8LxGyr1emB4ReHk90B7t_DB301cl5fz6H1EIBnUYkuOYIeCQXvqTmEHduR1KPumEYc6Jfw3kv1tY95k-bugObZ4FihLhWXw4ud8fXRl_CTnD3J3FSx-cn4K8mfy8JjTc1RDmEx5_4-TbBhPyTgj5EAXqB1ddUw7k46UAh_-w2G07SrOxsl1b57Etwp0gvuu4tkpXICYmG423n-RjVvtvuxjSzQyhUZ2Lmfbvi1tLlY7_uzTh_BwwWWYLdJtnmKEblmGReoAu_Qs6A";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJvcGVyYXRvciJ9fQ.eEQp3HyUFsV1bRBDvww3DirCY1R-vrASYT3KXnTeXBa8Owuag8Mc1I_v93XBatf-t-Y0qd6r9uNQuRiRpuXkrC01MJwyPnyvKDYHFAX828PIMdFgZ5FUGU0S6r1B4B8FaVZnfMdwyyQW9tCeFBvvh2hkuodoOlkcaJnR98kMrYjGHVoyDQc5H5JnU5O8Kkb9STE-XR-3b8VdOlGJR-ljX4Vw8Fipo5p7fo_VdhhUXD2C974DrbQWtsXhqUTqOFWAEUcUMM2ODH8pEFWhG8poHVP8LLWCcSFxZDN_Ia3dNR8OK9SEblCPIlfimiMtscqxli-9uC00n62UmLuQtGVlXA";

// Per-call unique IDs to prevent PK conflicts across runs.
const TestFixtures = struct {
    agent_active: []const u8,
    agent_empty: []const u8,
    agent_nonexistent: []const u8,

    fn deinit(self: TestFixtures, alloc: std.mem.Allocator) void {
        alloc.free(self.agent_active);
        alloc.free(self.agent_empty);
        alloc.free(self.agent_nonexistent);
    }
};

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn makeFixtures(alloc: std.mem.Allocator) !TestFixtures {
    const active = try id_format.generateAgentId(alloc);
    errdefer alloc.free(active);
    const empty = try id_format.generateAgentId(alloc);
    errdefer alloc.free(empty);
    const nonexistent = try id_format.generateAgentId(alloc);
    return .{ .agent_active = active, .agent_empty = empty, .agent_nonexistent = nonexistent };
}

fn seedWorkspace(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'DashTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1, $3, 'dash_test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms, TEST_BALANCE_NANOS });
}

fn seedAgents(conn: *pg.Conn, alloc: std.mem.Allocator, fx: TestFixtures, now_ms: i64) !void {
    const agents = [_]struct { id: []const u8, suffix: []const u8 }{
        .{ .id = fx.agent_active, .suffix = "active" },
        .{ .id = fx.agent_empty, .suffix = "empty" },
    };
    for (agents) |z| {
        // Derive name from the unique agent id so two test functions in the
        // same run don't collide on UNIQUE (workspace_id, name).
        const name = try std.fmt.allocPrint(alloc, "agent-dash-{s}-{s}", .{ z.suffix, z.id });
        defer alloc.free(name);
        _ = try conn.exec(
            \\INSERT INTO core.agents
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ z.id, TEST_WORKSPACE_ID, name, now_ms });
    }
}

test "integration: dashboard kill switch — transitions, 409, 404" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const fx = try makeFixtures(alloc);
    defer fx.deinit(alloc);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);
    try seedAgents(conn, alloc, fx, now_ms);

    const stop_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents/{s}", .{ TEST_WORKSPACE_ID, fx.agent_active });
    defer alloc.free(stop_url);
    const stop_body = "{\"status\":\"stopped\"}";

    { // T5: user role → 403
        var req = h.request(.PATCH, stop_url);
        req = try req.bearer(TOKEN_USER);
        req = try req.json(stop_body);
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T6: operator active → 200 status=stopped
        var req = h.request(.PATCH, stop_url);
        req = try req.bearer(TOKEN_OPERATOR);
        req = try req.json(stop_body);
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"status\":\"stopped\""));
    }
    { // re-stopping an already-stopped agent → 409 UZ-AGT-010 (no transition)
        var req = h.request(.PATCH, stop_url);
        req = try req.bearer(TOKEN_OPERATOR);
        req = try req.json(stop_body);
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains("UZ-AGT-010"));
    }
    { // nonexistent agent → 404 UZ-AGT-009
        const missing = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents/{s}", .{ TEST_WORKSPACE_ID, fx.agent_nonexistent });
        defer alloc.free(missing);
        var req = h.request(.PATCH, missing);
        req = try req.bearer(TOKEN_OPERATOR);
        req = try req.json(stop_body);
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try std.testing.expect(r.bodyContains("UZ-AGT-009"));
    }
}
