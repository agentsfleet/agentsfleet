// HTTP integration tests for the workspace-scoped /memories collection — now a
// READ-ONLY tenant surface (the write-verb teardown: the runner plane is
// the only writer).
//
//   GET    /v1/workspaces/{ws}/zombies/{zid}/memories          → list-or-search
//   POST   /v1/workspaces/{ws}/zombies/{zid}/memories          → retired (404/405)
//   DELETE /v1/workspaces/{ws}/zombies/{zid}/memories/{key}    → retired (404/405)
//
// Entries are seeded directly (memory_runtime INSERT) since POST is gone. Uses
// the shared TestHarness; DB-required; self-skips when TEST_DATABASE_URL is unset.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const id_format = @import("../../../types/id_format.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

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
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'MemoriesTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
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
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    // Memory is scoped by the raw zombie_id (UUID) after schema/013 — no zmb: form.
    _ = conn.exec(
        "DELETE FROM memory.memory_entries WHERE zombie_id IN ($1::uuid, $2::uuid)",
        .{ ZOMBIE_LOCAL, ZOMBIE_OTHER_WS },
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.zombies WHERE id IN ($1, $2)", .{ ZOMBIE_LOCAL, ZOMBIE_OTHER_WS }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// Seed one memory entry directly (the tenant write verbs are retired —
/// the runner push is the only writer; here we INSERT under the memory_runtime
/// role so the surviving GET surface has data to read).
fn seedEntry(f: Fixture, zombie_id: []const u8, key: []const u8, content: []const u8, category: []const u8) !void {
    const conn = try f.h.acquireConn();
    defer f.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("reset role ignored: {s}", .{@errorName(err)});
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    var id_buf: [128]u8 = undefined;
    const id = try std.fmt.bufPrint(&id_buf, "{s}:{s}", .{ zombie_id, key });
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, zombie_id, session_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, NULL, '1700000000', '1700000000')
        \\ON CONFLICT (key, zombie_id) DO UPDATE SET content = EXCLUDED.content, category = EXCLUDED.category
    , .{ uid, id, key, content, category, zombie_id });
}

fn memoriesUrl(ws: []const u8, zid: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/memories", .{ ws, zid });
}

fn memoryKeyUrl(ws: []const u8, zid: []const u8, key: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}/memories/{s}", .{ ws, zid, key });
}

// ── GET surface (the tenant memory API is read-only after the write-verb teardown) ──

test "integration: memories GET list returns a seeded entry" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, ZOMBIE_LOCAL, "goal:current", "ship the runner memory loop", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const list_r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer list_r.deinit();
    try list_r.expectStatus(.ok);
    try std.testing.expect(list_r.bodyContains("\"key\":\"goal:current\""));
    try std.testing.expect(list_r.bodyContains("ship the runner memory loop"));
}

test "integration: memories GET ?query= finds an entry by content match" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, ZOMBIE_LOCAL, "note:deploy", "deploy lands every monday morning", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=monday", .{url});
    defer ALLOC.free(search_url);
    const search_r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer search_r.deinit();
    try search_r.expectStatus(.ok);
    try std.testing.expect(search_r.bodyContains("\"key\":\"note:deploy\""));
}

// ── Memory-loss counters: the zero-hit search signal ──
// The harness server runs in-process, so the metrics globals asserted here are
// the same atomics the handler increments (backpressure-test precedent).

test "test_search_zero_hit_counts" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, ZOMBIE_LOCAL, "note:topic", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=nothing-matches-this", .{url});
    defer ALLOC.free(search_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total + 1, after.search_zero_hits_total);
}

test "test_search_hit_no_count" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, ZOMBIE_LOCAL, "note:fruit", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=kumquats", .{url});
    defer ALLOC.free(search_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"key\":\"note:fruit\""));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_list_never_counts_zero_hit" {
    const f = try fixture();
    defer f.deinit();
    // No seeded entries: the list path returns an empty set — still no count,
    // because only the ?query= search path is a recall-miss signal.
    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_category_filter_never_counts_zero_hit" {
    const f = try fixture();
    defer f.deinit();
    // The ?category= arm is a filtered list, not a search — an empty result
    // there must never read as a recall miss.
    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const cat_url = try std.fmt.allocPrint(ALLOC, "{s}?category=no-such-category", .{url});
    defer ALLOC.free(cat_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(cat_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_tenant_list_never_counts_drops" {
    const f = try fixture();
    defer f.deinit();
    // The tenant read is the passthrough Compactor arm — no window applies, so
    // the hydration-drop counters must never move on this surface.
    try seedEntry(f, ZOMBIE_LOCAL, "goal:current", "tenant reads are passthrough", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.hydration_dropped_entries_total, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total, after.hydration_dropped_bytes_total);
}

test "integration: memories GET without bearer returns 401" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try f.h.get(url).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

// ── Cross-workspace isolation on the surviving GET surface ──
//   (a) URL workspace = OTHER_WS → auth middleware rejects 403
//   (b) URL workspace = TEST_WS, zombie lives in OTHER_WS → handler 404 (no leak)

test "integration: memories GET cross-workspace URL returns 403" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(OTHER_WS_ID, ZOMBIE_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
}

test "integration: memories GET zombie-in-foreign-ws returns 404" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

// ── The tenant write verbs are retired (no compat shim) ──
// POST /memories and DELETE /memories/{key} were removed with the runner-push
// cutover — the runner plane is the only writer. Both 404/405; GET still 200.

test "integration: tenant memory POST is retired (404/405, no write surface)" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"k\",\"content\":\"c\",\"category\":\"core\"}",
    )).send();
    defer r.deinit();
    try std.testing.expect(r.status == 404 or r.status == 405);
}

test "integration: tenant memory DELETE is retired (404/405, no delete surface)" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoryKeyUrl(TEST_WORKSPACE_ID, ZOMBIE_LOCAL, "any");
    defer ALLOC.free(url);
    const r = try (try f.h.delete(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try std.testing.expect(r.status == 404 or r.status == 405);
}
