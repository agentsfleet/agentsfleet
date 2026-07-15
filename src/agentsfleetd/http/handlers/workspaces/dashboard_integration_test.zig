// HTTP integration tests for M12_001 dashboard endpoints (activity feed,
// fleet stop, billing summaries — per-fleet and per-workspace).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".
//
// Workspace and tenant IDs are fixed to match the embedded JWT tokens.
// Fleet, activity-event, and telemetry IDs are generated per call so
// concurrent or repeated runs never conflict on primary keys. No cleanup
// function is needed: make down && make up resets the DB between runs, and
// unique IDs within a run prevent PK collisions.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
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
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_USER = scope_fixtures.VIEWER;
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

// Per-call unique IDs to prevent PK conflicts across runs.
const TestFixtures = struct {
    fleet_active: []const u8,
    fleet_empty: []const u8,
    fleet_nonexistent: []const u8,

    fn deinit(self: TestFixtures, alloc: std.mem.Allocator) void {
        alloc.free(self.fleet_active);
        alloc.free(self.fleet_empty);
        alloc.free(self.fleet_nonexistent);
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
    const active = try id_format.generateFleetId(alloc);
    errdefer alloc.free(active);
    const empty = try id_format.generateFleetId(alloc);
    errdefer alloc.free(empty);
    const nonexistent = try id_format.generateFleetId(alloc);
    return .{ .fleet_active = active, .fleet_empty = empty, .fleet_nonexistent = nonexistent };
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

fn seedFleets(conn: *pg.Conn, alloc: std.mem.Allocator, fx: TestFixtures, now_ms: i64) !void {
    const fleets = [_]struct { id: []const u8, suffix: []const u8 }{
        .{ .id = fx.fleet_active, .suffix = "active" },
        .{ .id = fx.fleet_empty, .suffix = "empty" },
    };
    for (fleets) |z| {
        // Derive name from the unique fleet id so two test functions in the
        // same run don't collide on UNIQUE (workspace_id, name).
        const name = try std.fmt.allocPrint(alloc, "fleet-dash-{s}-{s}", .{ z.suffix, z.id });
        defer alloc.free(name);
        const config_json = try std.fmt.allocPrint(
            alloc,
            "{{\"name\":\"{s}\",\"x-agentsfleet\":{{\"triggers\":[{{\"type\":\"api\"}}],\"tools\":[],\"budget\":{{\"daily_dollars\":1.0}}}}}}",
            .{name},
        );
        defer alloc.free(config_json);
        _ = try conn.exec(
            \\INSERT INTO core.fleets
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, $4::jsonb, 'active', $5, $5)
        , .{ z.id, TEST_WORKSPACE_ID, name, config_json, now_ms });
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
    try seedFleets(conn, alloc, fx, now_ms);

    const stop_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, fx.fleet_active });
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
    { // re-stopping an already-stopped fleet → 409 UZ-AGT-010 (no transition)
        var req = h.request(.PATCH, stop_url);
        req = try req.bearer(TOKEN_OPERATOR);
        req = try req.json(stop_body);
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains("UZ-AGT-010"));
    }
    { // nonexistent fleet → 404 UZ-AGT-009
        const missing = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, fx.fleet_nonexistent });
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
