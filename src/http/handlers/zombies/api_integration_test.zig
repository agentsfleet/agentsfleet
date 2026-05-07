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
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
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

    // Full cursor round-trip: 5 zombies seeded, limit=2 means pages of
    // 2 + 2 + 1. Walk every page, accumulate ids, and assert:
    //   (a) continuation has no overlap with prior pages,
    //   (b) last page carries cursor=null,
    //   (c) union of ids across pages == seeded set (order agnostic).
    var seen_ids = std.StringHashMap(void).init(alloc);
    defer seen_ids.deinit();

    var next_cursor: ?[]const u8 = null;
    var page_count: usize = 0;
    while (page_count < 10) : (page_count += 1) { // hard cap guards runaway loop
        const url = if (next_cursor) |c|
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies?limit=2&cursor={s}", .{ TEST_WORKSPACE_ID, c })
        else
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies?limit=2", .{TEST_WORKSPACE_ID});
        defer alloc.free(url);
        if (next_cursor) |c| alloc.free(c);

        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const parsed = try std.json.parseFromSlice(std.json.Value, alloc, r.body, .{});
        defer parsed.deinit();

        const items = parsed.value.object.get("items").?.array;
        for (items.items) |item| {
            const id = item.object.get("id").?.string;
            const id_copy = try alloc.dupe(u8, id);
            const gop = try seen_ids.getOrPut(id_copy);
            try std.testing.expect(!gop.found_existing); // (a) no overlap across pages
        }

        const cursor_node = parsed.value.object.get("cursor").?;
        switch (cursor_node) {
            .null => {
                next_cursor = null;
                break; // (b) terminal page reached
            },
            .string => |s| next_cursor = try alloc.dupe(u8, s),
            else => return error.UnexpectedCursorType,
        }
    }
    try std.testing.expect(next_cursor == null);
    // (c) every seeded zombie was returned across the walk.
    try std.testing.expectEqual(@as(usize, 5), seen_ids.count());
    var seen_it = seen_ids.keyIterator();
    while (seen_it.next()) |key_ptr| alloc.free(key_ptr.*);

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

// Cross-file `name:` invariant: SKILL.md and TRIGGER.md must agree on identity.
// Handler enforcement at create.zig fires before workspace authorization, so a
// USER-role token still surfaces the mismatch error (no escalation needed).
test "integration: zombie create rejects SKILL/TRIGGER name mismatch with UZ-ZMB-011" {
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

    // SKILL.md says alpha-zombie; TRIGGER.md says beta-zombie. Both halves
    // parse cleanly in isolation — the rejection only fires at the install
    // handler, which is what this test pins.
    const body =
        "{\"source_markdown\":\"---\\nname: alpha-zombie\\ndescription: alpha\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: beta-zombie\\nx-usezombie:\\n  trigger:\\n    type: api\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-ZMB-011");
}
