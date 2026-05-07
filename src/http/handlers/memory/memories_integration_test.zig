// HTTP integration tests for the workspace-scoped /memories collection.
//
//   POST   /v1/workspaces/{ws}/zombies/{zid}/memories          → store
//   GET    /v1/workspaces/{ws}/zombies/{zid}/memories          → list-or-search
//   DELETE /v1/workspaces/{ws}/zombies/{zid}/memories/{key}    → idempotent 204
//
// Uses the shared TestHarness (src/http/test_harness.zig). DB-required;
// self-skips when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aff77";
const ZOMBIE_LOCAL = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc01";
const ZOMBIE_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc02";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

const Fixture = struct {
    h: *TestHarness,

    fn start() !Fixture {
        const h = try TestHarness.start(ALLOC, .{
            .configureRegistry = configureRegistry,
            .inline_jwks_json = TEST_JWKS,
            .issuer = TEST_ISSUER,
            .audience = TEST_AUDIENCE,
        });
        errdefer h.deinit();
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedTestData(conn);
        return .{ .h = h };
    }

    fn deinit(self: Fixture) void {
        if (self.h.acquireConn()) |c| {
            cleanupTestData(c);
            self.h.releaseConn(c);
        } else |_| {}
        self.h.deinit();
    }
};

fn fixture() !Fixture {
    return Fixture.start() catch |err| switch (err) {
        error.SkipZigTest => error.SkipZigTest,
        else => err,
    };
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'MemoriesTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'mem-local', '---\nname: mem-local\n---\ntest', '{"name":"mem-local"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_LOCAL, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'mem-other', '---\nname: mem-other\n---\ntest', '{"name":"mem-other"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch {};
    _ = conn.exec(
        "DELETE FROM memory.memory_entries WHERE instance_id IN ($1, $2)",
        .{ "zmb:" ++ ZOMBIE_LOCAL, "zmb:" ++ ZOMBIE_OTHER_WS },
    ) catch {};
    _ = conn.exec("RESET ROLE", .{}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE id IN ($1, $2)", .{ ZOMBIE_LOCAL, ZOMBIE_OTHER_WS }) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch {};
}

fn memoriesUrl(ws: []const u8, zid: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/memories", .{ ws, zid });
}

fn memoryKeyUrl(ws: []const u8, zid: []const u8, key: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/memories/{s}", .{ ws, zid, key });
}

// ── Happy path round-trip ───────────────────────────────────────────────────

test "integration: memories POST happy path returns 201 with key + category" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"contact:lead-acme\",\"content\":\"prefers email after 4pm\",\"category\":\"core\"}",
    )).send();
    defer r.deinit();
    try r.expectStatus(.created);
    try std.testing.expect(r.bodyContains("\"key\":\"contact:lead-acme\""));
    try std.testing.expect(r.bodyContains("\"category\":\"core\""));
}

test "integration: memories GET list returns previously stored entry" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const post_r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"goal:current\",\"content\":\"ship M41\",\"category\":\"core\"}",
    )).send();
    post_r.deinit();

    const list_r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer list_r.deinit();
    try list_r.expectStatus(.ok);
    try std.testing.expect(list_r.bodyContains("\"key\":\"goal:current\""));
    try std.testing.expect(list_r.bodyContains("\"content\":\"ship M41\""));
}

test "integration: memories GET ?query= finds entry by content match" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const post_r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"note:deploy\",\"content\":\"deploy lands every monday morning\"}",
    )).send();
    post_r.deinit();

    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=monday", .{url});
    defer ALLOC.free(search_url);
    const search_r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer search_r.deinit();
    try search_r.expectStatus(.ok);
    try std.testing.expect(search_r.bodyContains("\"key\":\"note:deploy\""));
}

test "integration: memories DELETE returns 204 with empty body" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const collection = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(collection);
    const post_r = try (try (try f.h.post(collection).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"ephemeral\",\"content\":\"temporary\"}",
    )).send();
    post_r.deinit();

    const del_url = try memoryKeyUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL, "ephemeral");
    defer ALLOC.free(del_url);
    const del_r = try (try f.h.delete(del_url).bearer(TOKEN_OPERATOR)).send();
    defer del_r.deinit();
    try del_r.expectStatus(.no_content);
    // RFC 9110 §6.4.5: 204 MUST NOT include a message body.
    try std.testing.expectEqual(@as(usize, 0), del_r.body.len);
}

test "integration: memories DELETE on missing key is idempotent 204" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoryKeyUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL, "never-existed");
    defer ALLOC.free(url);
    const r = try (try f.h.delete(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.no_content);
    try std.testing.expectEqual(@as(usize, 0), r.body.len);
}

// ── Validation / failure paths ─────────────────────────────────────────────

test "integration: memories POST without bearer returns 401" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try f.h.post(url).json("{\"key\":\"k\",\"content\":\"c\"}")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration: memories POST missing key field returns 400" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json("{\"content\":\"orphan\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

test "integration: memories POST missing content field returns 400" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json("{\"key\":\"orphan\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

test "integration: memories POST key containing '/' returns 400" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"folder/name\",\"content\":\"would orphan\"}",
    )).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

test "integration: memories POST oversized content returns 400" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const big = "{\"key\":\"k\",\"content\":\"" ++ "x" ** 16385 ++ "\"}";
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(big)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

// ── Cross-workspace isolation ──────────────────────────────────────────────
// Principal token's workspace = TEST_WORKSPACE_ID. Two failure shapes:
//   (a) URL workspace = OTHER_WS → auth middleware rejects with 403
//   (b) URL workspace = TEST_WS, zombie lives in OTHER_WS → handler returns
//       404 (don't-leak-existence pattern)

test "integration: memories POST cross-workspace URL returns 403" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(OTHER_WS_ID, ZOMBIE_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"k\",\"content\":\"c\"}",
    )).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
}

test "integration: memories POST zombie-in-foreign-ws returns 404" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"k\",\"content\":\"c\"}",
    )).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

test "integration: memories DELETE zombie-in-foreign-ws returns 404" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoryKeyUrl(TEST_WORKSPACE_ID, ZOMBIE_OTHER_WS, "any");
    defer ALLOC.free(url);
    const r = try (try f.h.delete(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

// ── Same-key overwrite invariant (PUT-like ON CONFLICT semantics) ─────────

test "integration: memories POST same key twice overwrites, GET reflects last write" {
    const f = fixture() catch |e| return e;
    defer f.deinit();

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);

    const r1 = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"contact:acme\",\"content\":\"first revision\"}",
    )).send();
    try r1.expectStatus(.created);
    r1.deinit();

    const r2 = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"contact:acme\",\"content\":\"second revision wins\"}",
    )).send();
    try r2.expectStatus(.created);
    r2.deinit();

    const list_r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer list_r.deinit();
    try list_r.expectStatus(.ok);
    try std.testing.expect(list_r.bodyContains("\"content\":\"second revision wins\""));
    try std.testing.expect(!list_r.bodyContains("\"content\":\"first revision\""));
}
