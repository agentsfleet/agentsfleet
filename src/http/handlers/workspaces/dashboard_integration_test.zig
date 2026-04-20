// HTTP integration tests for M12_001 dashboard endpoints (activity feed,
// zombie stop, billing summaries — per-zombie and per-workspace).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_WORKSPACE_OTHER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
const TEST_ZOMBIE_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001";
const TEST_ZOMBIE_EMPTY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002";
const TEST_ZOMBIE_NONEXISTENT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7999";
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

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID});
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'M12Test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json,
        \\   scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff3', $1, 'FREE', 2, 8,
        \\        false, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4', $1, 'FREE', 'free', 'ACTIVE', 'noop', 'sub-m12', $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });

    const zombie_specs = [_]struct { id: []const u8, name: []const u8 }{
        .{ .id = TEST_ZOMBIE_ACTIVE, .name = "zombie-m12-active" },
        .{ .id = TEST_ZOMBIE_EMPTY, .name = "zombie-m12-empty" },
    };
    for (zombie_specs) |z| {
        _ = try conn.exec(
            \\INSERT INTO core.zombies
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ z.id, TEST_WORKSPACE_ID, z.name, now_ms });
    }

    const activity_ids = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7100",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7102",
    };
    for (activity_ids, 0..) |aid, i| {
        _ = try conn.exec(
            \\INSERT INTO core.activity_events
            \\  (id, zombie_id, workspace_id, event_type, detail, created_at)
            \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'event_received', 'seed', $4)
        , .{ aid, TEST_ZOMBIE_ACTIVE, TEST_WORKSPACE_ID, now_ms - @as(i64, @intCast(i * 1000)) });
    }

    const telemetry_rows = [_]struct { tid: []const u8, eid: []const u8, cents: i64 }{
        .{ .tid = "tel-m12-0", .eid = "ev-m12-0", .cents = 500 },
        .{ .tid = "tel-m12-1", .eid = "ev-m12-1", .cents = 500 },
        .{ .tid = "tel-m12-2", .eid = "ev-m12-2", .cents = 0 },
    };
    for (telemetry_rows) |row| {
        _ = try conn.exec(
            \\INSERT INTO zombie_execution_telemetry
            \\  (id, zombie_id, workspace_id, event_id, token_count, time_to_first_token_ms,
            \\   epoch_wall_time_ms, wall_seconds, plan_tier, credit_deducted_cents, recorded_at)
            \\VALUES ($1, $2, $3, $4, 100, 42, $5, 3, 'free', $6, $5)
        , .{ row.tid, TEST_ZOMBIE_ACTIVE, TEST_WORKSPACE_ID, row.eid, now_ms, row.cents });
    }
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
}

// ── T1–T4: GET /v1/workspaces/{ws}/activity ─────────────────────────────────

test "integration: dashboard activity — auth, seed, invalid cursor" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/activity", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    { // T1: happy path
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

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── T5–T7: POST /v1/workspaces/{ws}/zombies/{id}/stop ────────────────────────

test "integration: dashboard zombie stop — transitions, 409, 404" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const stop_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/stop", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_ACTIVE });
    defer alloc.free(stop_url);

    { // user role → 403 (kill switch requires operator)
        const r = try (try h.post(stop_url).bearer(TOKEN_USER)).rawBody("").send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // operator active → 200, status=stopped
        const r = try (try h.post(stop_url).bearer(TOKEN_OPERATOR)).rawBody("").send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"status\":\"stopped\""));
    }
    { // re-call → 409 UZ-ZMB-010
        const r = try (try h.post(stop_url).bearer(TOKEN_OPERATOR)).rawBody("").send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains("UZ-ZMB-010"));
    }
    { // nonexistent zombie → 404 UZ-ZMB-009
        const missing = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/stop", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_NONEXISTENT });
        defer alloc.free(missing);
        const r = try (try h.post(missing).bearer(TOKEN_OPERATOR)).rawBody("").send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try std.testing.expect(r.bodyContains("UZ-ZMB-009"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── T8–T10: GET /v1/workspaces/{ws}/zombies/{id}/billing/summary ──────────────

test "integration: dashboard per-zombie billing summary — populated, zeros, IDOR" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url_active = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/billing/summary?period_days=30", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_ACTIVE });
    defer alloc.free(url_active);
    { // user role → 403 (RULE BIL)
        const r = try (try h.get(url_active).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // operator — 2 billable + 1 non-billable → total_runs=3, total_cents=1000
        const r = try (try h.get(url_active).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"total_runs\":3"));
        try std.testing.expect(r.bodyContains("\"total_cents\":1000"));
    }
    { // empty zombie → 200 zeros (not 404)
        const url_empty = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/billing/summary?period_days=7", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_EMPTY });
        defer alloc.free(url_empty);
        const r = try (try h.get(url_empty).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"total_runs\":0"));
        try std.testing.expect(r.bodyContains("\"total_cents\":0"));
    }
    { // nonexistent zombie → 404 UZ-ZMB-009
        const url_missing = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/billing/summary", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_NONEXISTENT });
        defer alloc.free(url_missing);
        const r = try (try h.get(url_missing).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try std.testing.expect(r.bodyContains("UZ-ZMB-009"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

// ── T11: workspace billing summary surfaces aggregated telemetry ──────────────

test "integration: dashboard workspace billing summary surfaces real telemetry" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/billing/summary?period_days=30", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total_runs\":3"));
    try std.testing.expect(r.bodyContains("\"total_cents\":1000"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
