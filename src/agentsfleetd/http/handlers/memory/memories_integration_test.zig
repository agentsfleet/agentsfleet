// HTTP integration tests for the workspace-scoped /memories collection — now a
// READ-ONLY tenant surface (the write-verb teardown: the runner plane is
// the only writer).
//
//   GET    /v1/workspaces/{ws}/agents/{zid}/memories          → list-or-search
//   POST   /v1/workspaces/{ws}/agents/{zid}/memories          → retired (404/405)
//   DELETE /v1/workspaces/{ws}/agents/{zid}/memories/{key}    → retired (404/405)
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
const AGENTSFLEET_LOCAL = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc01";
const AGENTSFLEET_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc02";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJvcGVyYXRvciJ9fQ.eEQp3HyUFsV1bRBDvww3DirCY1R-vrASYT3KXnTeXBa8Owuag8Mc1I_v93XBatf-t-Y0qd6r9uNQuRiRpuXkrC01MJwyPnyvKDYHFAX828PIMdFgZ5FUGU0S6r1B4B8FaVZnfMdwyyQW9tCeFBvvh2hkuodoOlkcaJnR98kMrYjGHVoyDQc5H5JnU5O8Kkb9STE-XR-3b8VdOlGJR-ljX4Vw8Fipo5p7fo_VdhhUXD2C974DrbQWtsXhqUTqOFWAEUcUMM2ODH8pEFWhG8poHVP8LLWCcSFxZDN_Ia3dNR8OK9SEblCPIlfimiMtscqxli-9uC00n62UmLuQtGVlXA";

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
        \\INSERT INTO core.agents (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'mem-local', '---\nname: mem-local\n---\ntest', '{"name":"mem-local"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_LOCAL, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.agents (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'mem-other', '---\nname: mem-other\n---\ntest', '{"name":"mem-other"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    // Memory is scoped by the raw agent_id (UUID) after schema/013 — no legacy instance_id prefix.
    _ = conn.exec(
        "DELETE FROM memory.memory_entries WHERE agent_id IN ($1::uuid, $2::uuid)",
        .{ AGENTSFLEET_LOCAL, AGENTSFLEET_OTHER_WS },
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.agents WHERE id IN ($1, $2)", .{ AGENTSFLEET_LOCAL, AGENTSFLEET_OTHER_WS }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// Every seeded row's updated_at (epoch ms) — pinned so the tenant-wire test
/// can assert the exact unquoted JSON number.
const SEED_TS_MS: i64 = 1_700_000_000_000;

/// Seed one memory entry directly (the tenant write verbs are retired —
/// the runner push is the only writer; here we INSERT under the memory_runtime
/// role so the surviving GET surface has data to read).
fn seedEntry(f: Fixture, agent_id: []const u8, key: []const u8, content: []const u8, category: []const u8) !void {
    const conn = try f.h.acquireConn();
    defer f.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("reset role ignored: {s}", .{@errorName(err)});
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    var id_buf: [128]u8 = undefined;
    const id = try std.fmt.bufPrint(&id_buf, "{s}:{s}", .{ agent_id, key });
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, agent_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, $7, $7)
        \\ON CONFLICT (key, agent_id) DO UPDATE SET content = EXCLUDED.content, category = EXCLUDED.category
    , .{ uid, id, key, content, category, agent_id, SEED_TS_MS });
}

fn memoriesUrl(ws: []const u8, zid: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/agents/{s}/memories", .{ ws, zid });
}

fn memoryKeyUrl(ws: []const u8, zid: []const u8, key: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/agents/{s}/memories/{s}", .{ ws, zid, key });
}

// ── GET surface (the tenant memory API is read-only after the write-verb teardown) ──

test "integration: memories GET list returns a seeded entry" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "ship the runner memory loop", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const list_r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer list_r.deinit();
    try list_r.expectStatus(.ok);
    try std.testing.expect(list_r.bodyContains("\"key\":\"goal:current\""));
    try std.testing.expect(list_r.bodyContains("ship the runner memory loop"));
}

test "integration: tenant memory updated_at is a JSON number (epoch millis)" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "numeric wire shape", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // Unquoted digits after the field name = a JSON number on the wire — the
    // exact seeded epoch-millis value, never a decimal-string shape.
    try std.testing.expect(r.bodyContains("\"updated_at\":1700000000000"));
    try std.testing.expect(!r.bodyContains("\"updated_at\":\""));
}

test "integration: memories GET ?query= finds an entry by content match" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:deploy", "deploy lands every monday morning", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:topic", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:fruit", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "tenant reads are passthrough", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const r = try f.h.get(url).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

// ── Cross-workspace isolation on the surviving GET surface ──
//   (a) URL workspace = OTHER_WS → auth middleware rejects 403
//   (b) URL workspace = TEST_WS, agent lives in OTHER_WS → handler 404 (no leak)

test "integration: memories GET cross-workspace URL returns 403" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(OTHER_WS_ID, AGENTSFLEET_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
}

test "integration: memories GET agent-in-foreign-ws returns 404" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_OTHER_WS);
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
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
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
    const url = try memoryKeyUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL, "any");
    defer ALLOC.free(url);
    const r = try (try f.h.delete(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try std.testing.expect(r.status == 404 or r.status == 405);
}
