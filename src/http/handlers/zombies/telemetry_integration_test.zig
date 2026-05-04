// HTTP integration tests for M18_001 execution telemetry endpoints.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.
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
const TEST_ZOMBIE_ID = "zombie-m18-http-test";
const TEST_REPO_URL = "https://github.com/usezombie/m18-http-test";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";

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
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'M18Test', $2, $2)
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
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff1', $1, 'FREE', 2, 8,
        \\        false, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, plan_tier, plan_sku, balance_cents, grant_source, created_at, updated_at)
        \\VALUES ($1, 'free', 'free_default', 1000, 'telemetry_test_seed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
}

// ── Customer endpoint auth, limit, and cursor enforcement ─────────────────

test "integration: customer telemetry endpoint auth, limit and cursor enforcement" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const ws_base = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/telemetry", .{ TEST_WORKSPACE_ID, TEST_ZOMBIE_ID });
    defer alloc.free(ws_base);

    { // T6: valid JWT, correct workspace → 200
        const r = try (try h.get(ws_base).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }
    { // T7: JWT workspace ≠ path workspace → 403
        const wrong_ws = try std.fmt.allocPrint(alloc, "/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99/zombies/{s}/telemetry", .{TEST_ZOMBIE_ID});
        defer alloc.free(wrong_ws);
        const r = try (try h.get(wrong_ws).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T8: limit=0 → 400
        const url = try std.fmt.allocPrint(alloc, "{s}?limit=0", .{ws_base});
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T9: limit=201 → 400
        const url = try std.fmt.allocPrint(alloc, "{s}?limit=201", .{ws_base});
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T10: invalid base64 cursor → 400
        const url = try std.fmt.allocPrint(alloc, "{s}?cursor=!!invalid!!", .{ws_base});
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}
