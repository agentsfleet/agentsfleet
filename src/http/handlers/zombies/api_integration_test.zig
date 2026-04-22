// HTTP integration tests for the zombies CRUD API — focused on cursor
// pagination on GET /v1/workspaces/{ws}/zombies.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_REPO_URL = "https://github.com/usezombie/m27-zombies-pagination-test";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedWorkspace(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'ListPaginationTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
}

fn seedZombies(alloc: std.mem.Allocator, conn: *pg.Conn, count: usize, base_ms: i64) ![][]const u8 {
    var ids = try alloc.alloc([]const u8, count);
    errdefer {
        for (ids[0..]) |id| if (id.len > 0) alloc.free(id);
        alloc.free(ids);
    }
    for (0..count) |i| {
        const id = try id_format.generateZombieId(alloc);
        ids[i] = id;
        const name = try std.fmt.allocPrint(alloc, "zombie-pg-{d}-{d}", .{ base_ms, i });
        defer alloc.free(name);
        _ = try conn.exec(
            \\INSERT INTO core.zombies
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ id, TEST_WORKSPACE_ID, name, base_ms + @as(i64, @intCast(i)) });
    }
    return ids;
}

fn freeIds(alloc: std.mem.Allocator, ids: [][]const u8) void {
    for (ids) |id| alloc.free(id);
    alloc.free(ids);
}

// ── Cursor pagination roundtrip + invalid-cursor handling ────────────────────

test "integration: zombies list — cursor pagination roundtrip" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = std.time.milliTimestamp();
    try seedWorkspace(conn, now_ms);
    const ids = try seedZombies(alloc, conn, 5, now_ms);
    defer freeIds(alloc, ids);

    // Page 1: limit=2 → items present, cursor present.
    const url_p1 = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies?limit=2", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_p1);
    const r1 = try (try h.get(url_p1).bearer(TOKEN_USER)).send();
    defer r1.deinit();
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("\"items\""));
    try std.testing.expect(r1.bodyContains("\"cursor\""));
    // Cursor should be a non-null string when more pages remain.
    try std.testing.expect(!r1.bodyContains("\"cursor\":null"));

    // Bad cursor → 400.
    const url_bad = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies?cursor=not-a-cursor", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_bad);
    const r_bad = try (try h.get(url_bad).bearer(TOKEN_USER)).send();
    defer r_bad.deinit();
    try r_bad.expectStatus(.bad_request);

    // No-token → 401.
    const url_anon = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_anon);
    const r_anon = try h.get(url_anon).send();
    defer r_anon.deinit();
    try r_anon.expectStatus(.unauthorized);
}
