// HTTP integration tests for M12_001 dashboard endpoints (activity feed,
// zombie stop, billing summaries — per-zombie and per-workspace).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".
//
// Workspace and tenant IDs are fixed to match the embedded JWT tokens.
// Zombie, activity-event, and telemetry IDs are generated per call so
// concurrent or repeated runs never conflict on primary keys. No cleanup
// function is needed: make down && make up resets the DB between runs, and
// unique IDs within a run prevent PK collisions.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Fixed — embedded in TOKEN_USER and TOKEN_OPERATOR.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_WORKSPACE_OTHER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
const TEST_REPO_URL = "https://github.com/usezombie/m12-http-test";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

// Per-call unique IDs to prevent PK conflicts across runs.
const TestFixtures = struct {
    zombie_active: []const u8,
    zombie_empty: []const u8,
    zombie_nonexistent: []const u8,

    fn deinit(self: TestFixtures, alloc: std.mem.Allocator) void {
        alloc.free(self.zombie_active);
        alloc.free(self.zombie_empty);
        alloc.free(self.zombie_nonexistent);
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
    const active = try id_format.generateZombieId(alloc);
    errdefer alloc.free(active);
    const empty = try id_format.generateZombieId(alloc);
    errdefer alloc.free(empty);
    const nonexistent = try id_format.generateZombieId(alloc);
    return .{ .zombie_active = active, .zombie_empty = empty, .zombie_nonexistent = nonexistent };
}

fn seedWorkspace(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'DashTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json,
        \\   scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff3', $1, 'FREE', 2, 8,
        \\        false, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, plan_tier, plan_sku, balance_cents, grant_source, created_at, updated_at)
        \\VALUES ($1, 'free', 'free_default', 1000, 'dash_test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
}

fn seedZombies(conn: *pg.Conn, alloc: std.mem.Allocator, fx: TestFixtures, now_ms: i64) !void {
    const zombies = [_]struct { id: []const u8, suffix: []const u8 }{
        .{ .id = fx.zombie_active, .suffix = "active" },
        .{ .id = fx.zombie_empty, .suffix = "empty" },
    };
    for (zombies) |z| {
        // Derive name from the unique zombie id so two test functions in the
        // same run don't collide on UNIQUE (workspace_id, name).
        const name = try std.fmt.allocPrint(alloc, "zombie-dash-{s}-{s}", .{ z.suffix, z.id });
        defer alloc.free(name);
        _ = try conn.exec(
            \\INSERT INTO core.zombies
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ z.id, TEST_WORKSPACE_ID, name, now_ms });
    }
}

fn seedActivityEvents(conn: *pg.Conn, alloc: std.mem.Allocator, zombie_id: []const u8, now_ms: i64) !void {
    for (0..3) |i| {
        const aid = try id_format.allocUuidV7(alloc);
        defer alloc.free(aid);
        _ = try conn.exec(
            \\INSERT INTO core.activity_events
            \\  (id, zombie_id, workspace_id, event_type, detail, created_at)
            \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'event_received', 'seed', $4)
        , .{ aid, zombie_id, TEST_WORKSPACE_ID, now_ms - @as(i64, @intCast(i * 1000)) });
    }
}


// ── T1–T4: GET /v1/workspaces/{ws}/activity ──────────────────────────────────

test "integration: dashboard activity — auth, seed, invalid cursor" {
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
    const now_ms = std.time.milliTimestamp();
    try seedWorkspace(conn, now_ms);
    try seedZombies(conn, alloc, fx, now_ms);
    try seedActivityEvents(conn, alloc, fx.zombie_active, now_ms);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/activity", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    { // T1: happy path — events key always present
        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("events"));
    }
    { // T2: no token → 401
        const r = try h.get(url).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    { // T3: wrong workspace → 403
        const wrong = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/activity", .{TEST_WORKSPACE_OTHER});
        defer alloc.free(wrong);
        const r = try (try h.get(wrong).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T4: invalid cursor → 400
        const bad = try std.fmt.allocPrint(alloc, "{s}?cursor=!!not-base64!!", .{url});
        defer alloc.free(bad);
        const r = try (try h.get(bad).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
}

// ── T5–T7: DELETE /v1/workspaces/{ws}/zombies/{id}/current-run ───────────────
// T8–T11 (billing/summary) removed: those routes are intentionally absent from
// the router (pre-v2.0 policy — router.zig test "rejects removed workspace
// billing routes"). Billing display uses /v1/tenants/me/billing instead.

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
    const now_ms = std.time.milliTimestamp();
    try seedWorkspace(conn, now_ms);
    try seedZombies(conn, alloc, fx, now_ms);

    const kill_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/current-run", .{ TEST_WORKSPACE_ID, fx.zombie_active });
    defer alloc.free(kill_url);

    { // T5: user role → 403
        const r = try (try h.delete(kill_url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T6: operator active → 200 status=stopped
        const r = try (try h.delete(kill_url).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"status\":\"stopped\""));
    }
    { // T7: re-call on stopped zombie → 409 UZ-ZMB-010
        const r = try (try h.delete(kill_url).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains("UZ-ZMB-010"));
    }
    { // nonexistent zombie → 404 UZ-ZMB-009
        const missing = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/current-run", .{ TEST_WORKSPACE_ID, fx.zombie_nonexistent });
        defer alloc.free(missing);
        const r = try (try h.delete(missing).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try std.testing.expect(r.bodyContains("UZ-ZMB-009"));
    }
}
