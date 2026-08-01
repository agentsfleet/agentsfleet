// Integration tests for /v1/api-keys.
//
// Covers:
//   - POST creates: 201, agt_t prefix, SHA-256 hex persisted in core.api_keys.
//   - Duplicate key_name within a tenant: 409 UZ-APIKEY-005.
//   - Round-trip auth: a minted agt_t key authenticates a subsequent GET.
//   - PATCH {active:false} revokes; the same key can no longer authenticate.
//   - Re-revoke is 409; DELETE on active/revoked/missing keys is 409/204/404.
//   - Tenant isolation: GET as tenant A does not return tenant B's rows.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".
//
// The operator JWT and JWKS mirror cross_workspace_idor_test.zig so any future
// key rotation in that test updates both at once — do NOT regenerate here.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const api_key_lookup = @import("../../../cmd/api_key_lookup.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc01";
const FOREIGN_KEY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc02";

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

// Real DB-backed api-key lookup. The ctx must outlive the middleware chain, so
// we park it at module scope — `zig build test` runs tests sequentially in a
// single process, so reassigning across tests is safe (each reassignment
// happens after the previous harness's deinit sets the chain pointer stale).
// If the test runner ever parallelizes, move into TestHarness as an extension.
// SAFETY: test fixture; field is populated by the surrounding builder before any read.
var api_key_ctx: api_key_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    api_key_ctx = .{ .pool = h.pool };
    reg.tenant_api_key_mw = .{ .host = &api_key_ctx, .lookup = api_key_lookup.lookup };
}

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
    cleanupApiKeys(conn); // start with a clean key set
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'ApiKeysTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'ApiKeysOtherTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ OTHER_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
}

fn cleanupApiKeys(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.api_keys WHERE tenant_id IN ($1::uuid, $2::uuid)", .{ TEST_TENANT_ID, OTHER_TENANT_ID }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn finalCleanup(h: *TestHarness) void {
    if (h.acquireConn()) |c| {
        cleanupApiKeys(c);
        h.releaseConn(c);
    } else |_| {}
}

fn parseJsonString(alloc: std.mem.Allocator, body: []const u8, field: []const u8) !?[]const u8 {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object.get(field) orelse return null;
    if (obj != .string) return null;
    return try alloc.dupe(u8, obj.string);
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "integration: POST /v1/api-keys returns 201 with agt_t key and persists SHA-256 hash" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"ci-pipeline\",\"description\":\"GH Actions\"}")).send();
    defer resp.deinit();
    try resp.expectStatus(.created);

    const raw_key = (try parseJsonString(ALLOC, resp.body, "key")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(raw_key);
    try std.testing.expect(std.mem.startsWith(u8, raw_key, auth_mw.tenant_api_key.TENANT_KEY_PREFIX));
    try std.testing.expectEqual(@as(usize, auth_mw.tenant_api_key.TENANT_KEY_PREFIX.len + 64), raw_key.len);

    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(raw_key, &digest, .{});
    const expected_hex = std.fmt.bytesToHex(digest, .lower);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT key_hash FROM core.api_keys WHERE tenant_id = $1::uuid AND key_name = $2
    , .{ TEST_TENANT_ID, "ci-pipeline" }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    const stored_hash = try row.get([]u8, 0);
    try std.testing.expectEqualStrings(expected_hex[0..], stored_hash);
    finalCleanup(h);
}

test "integration: POST /v1/api-keys duplicate key_name returns 409 UZ-APIKEY-005" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const first = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"duplicate-name\"}")).send();
    defer first.deinit();
    try first.expectStatus(.created);

    const second = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"duplicate-name\"}")).send();
    defer second.deinit();
    try second.expectStatus(.conflict);
    try std.testing.expect(second.bodyContains("UZ-APIKEY-005"));
    finalCleanup(h);
}

test "integration: minted agt_t key authenticates GET, revoked by PATCH {active:false}" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const create_resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"round-trip\"}")).send();
    defer create_resp.deinit();
    try create_resp.expectStatus(.created);
    const raw_key = (try parseJsonString(ALLOC, create_resp.body, "key")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(raw_key);
    const id = (try parseJsonString(ALLOC, create_resp.body, "id")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(id);

    // Authenticate GET using the minted raw key (Bearer <agt_t...>).
    const list_before = try (try h.get("/v1/api-keys").bearer(raw_key)).send();
    defer list_before.deinit();
    try list_before.expectStatus(.ok);
    try std.testing.expect(!list_before.bodyContains("key_hash"));
    try std.testing.expect(!list_before.bodyContains("\"last_used_at\":null"));

    // Revoke via PATCH.
    const patch_url = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{id});
    defer ALLOC.free(patch_url);
    const patch_resp = try (try (try h.request(.PATCH, patch_url).bearer(TOKEN_OPERATOR))
        .json("{\"active\":false}")).send();
    defer patch_resp.deinit();
    try patch_resp.expectStatus(.ok);

    // Revoked key no longer authenticates.
    const list_after = try (try h.get("/v1/api-keys").bearer(raw_key)).send();
    defer list_after.deinit();
    try list_after.expectStatus(.unauthorized);
    try std.testing.expect(list_after.bodyContains("UZ-APIKEY-004"));
    finalCleanup(h);
}

test "integration: PATCH already-revoked key returns 409 UZ-APIKEY-006" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const create_resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"already-revoked\"}")).send();
    defer create_resp.deinit();
    try create_resp.expectStatus(.created);
    const id = (try parseJsonString(ALLOC, create_resp.body, "id")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(id);

    const patch_path = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{id});
    defer ALLOC.free(patch_path);

    const first_revoke = try (try (try h.request(.PATCH, patch_path).bearer(TOKEN_OPERATOR))
        .json("{\"active\":false}")).send();
    defer first_revoke.deinit();
    try first_revoke.expectStatus(.ok);

    const second_revoke = try (try (try h.request(.PATCH, patch_path).bearer(TOKEN_OPERATOR))
        .json("{\"active\":false}")).send();
    defer second_revoke.deinit();
    try second_revoke.expectStatus(.conflict);
    try std.testing.expect(second_revoke.bodyContains("UZ-APIKEY-006"));
    finalCleanup(h);
}

test "integration: DELETE active key → 409, revoked key → 204, missing key → 404" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const create_resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"delete-flow\"}")).send();
    defer create_resp.deinit();
    try create_resp.expectStatus(.created);
    const id = (try parseJsonString(ALLOC, create_resp.body, "id")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(id);

    const del_path = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{id});
    defer ALLOC.free(del_path);

    const del_active = try (try h.delete(del_path).bearer(TOKEN_OPERATOR)).send();
    defer del_active.deinit();
    try del_active.expectStatus(.conflict);

    const patch_resp = try (try (try h.request(.PATCH, del_path).bearer(TOKEN_OPERATOR))
        .json("{\"active\":false}")).send();
    defer patch_resp.deinit();
    try patch_resp.expectStatus(.ok);

    const del_revoked = try (try h.delete(del_path).bearer(TOKEN_OPERATOR)).send();
    defer del_revoked.deinit();
    try del_revoked.expectStatus(.no_content);

    const del_missing = try (try h.delete(del_path).bearer(TOKEN_OPERATOR)).send();
    defer del_missing.deinit();
    try del_missing.expectStatus(.not_found);
    try std.testing.expect(del_missing.bodyContains("UZ-APIKEY-003"));
    finalCleanup(h);
}

test "integration: GET /v1/api-keys returns only the calling tenant's rows" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Seed a key directly into OTHER_TENANT_ID (no JWT exists for that tenant).
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec(
            \\INSERT INTO core.api_keys (id, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, 'other-tenant-key', '', 'deadbeef' , 'user_other', TRUE, $3, $3)
        , .{ FOREIGN_KEY_ID, OTHER_TENANT_ID, clock.nowMillis() });
    }

    // Operator for TEST_TENANT_ID mints one key of their own.
    const create_resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR))
        .json("{\"key_name\":\"own-tenant-key\"}")).send();
    defer create_resp.deinit();
    try create_resp.expectStatus(.created);

    // Listing as TEST_TENANT_ID must NOT reveal OTHER_TENANT_ID's row.
    const list_resp = try (try h.get("/v1/api-keys").bearer(TOKEN_OPERATOR)).send();
    defer list_resp.deinit();
    try list_resp.expectStatus(.ok);
    try std.testing.expect(list_resp.bodyContains("own-tenant-key"));
    try std.testing.expect(!list_resp.bodyContains("other-tenant-key"));
    try std.testing.expect(!list_resp.bodyContains(FOREIGN_KEY_ID));
    finalCleanup(h);
}

test "integration: GET /v1/api-keys rejects malformed pagination params with 400" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // page_size is a retired parameter: refused outright (400 UZ-REQ-001),
    // never silently ignored.
    const bad_size = try (try h.get("/v1/api-keys?page_size=abc").bearer(TOKEN_OPERATOR)).send();
    defer bad_size.deinit();
    try bad_size.expectStatus(.bad_request);

    // page is equally retired — refused, not clamped or defaulted.
    const bad_page = try (try h.get("/v1/api-keys?page=0").bearer(TOKEN_OPERATOR)).send();
    defer bad_page.deinit();
    try bad_page.expectStatus(.bad_request);
    finalCleanup(h);
}

// ── Keyset paging with the sort allowlist ───────────────────────────────────

fn mintKey(h: *TestHarness, key_name: []const u8) !void {
    const body = try std.fmt.allocPrint(ALLOC, "{{\"key_name\":\"{s}\"}}", .{key_name});
    defer ALLOC.free(body);
    const resp = try (try (try h.post("/v1/api-keys").bearer(TOKEN_OPERATOR)).json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
}

/// Walk the key list to exhaustion under `sort`, returning ids in arrival
/// order and asserting every non-final page is `limit` long.
fn walkKeys(h: *TestHarness, sort: []const u8, limit: usize) !std.ArrayList([]const u8) {
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (ids.items) |id| ALLOC.free(id);
        ids.deinit(ALLOC);
    }
    var cursor: ?[]const u8 = null;
    defer if (cursor) |c| ALLOC.free(c);
    while (true) {
        const path = if (cursor) |c|
            try std.fmt.allocPrint(ALLOC, "/v1/api-keys?sort={s}&limit={d}&starting_after={s}", .{ sort, limit, c })
        else
            try std.fmt.allocPrint(ALLOC, "/v1/api-keys?sort={s}&limit={d}", .{ sort, limit });
        defer ALLOC.free(path);
        const resp = try (try h.get(path).bearer(TOKEN_OPERATOR)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
        defer parsed.deinit();
        const obj = parsed.value.object;
        const items = obj.get("items").?.array;
        for (items.items) |item| {
            try ids.append(ALLOC, try ALLOC.dupe(u8, item.object.get("id").?.string));
        }
        const next = obj.get("next_cursor").?;
        if (next == .null) break;
        try std.testing.expectEqual(limit, items.items.len);
        if (cursor) |c| ALLOC.free(c);
        cursor = try ALLOC.dupe(u8, next.string);
    }
    return ids;
}

fn freeWalkedIds(ids: *std.ArrayList([]const u8)) void {
    for (ids.items) |id| ALLOC.free(id);
    ids.deinit(ALLOC);
}

test "integration: test_api_keys_list_uses_keyset_envelope_and_keeps_sorts" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try mintKey(h, "sort-b-key");
    try mintKey(h, "sort-a-key");
    try mintKey(h, "sort-c-key");

    const list_resp = try (try h.get("/v1/api-keys?limit=10").bearer(TOKEN_OPERATOR)).send();
    defer list_resp.deinit();
    try list_resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, list_resp.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqual(@as(usize, 3), obj.count());
    try std.testing.expect(obj.contains("items"));
    try std.testing.expect(obj.contains("total"));
    try std.testing.expect(obj.contains("next_cursor"));

    // The retired page spelling is refused.
    const retired = try (try h.get("/v1/api-keys?page=2").bearer(TOKEN_OPERATOR)).send();
    defer retired.deinit();
    try retired.expectStatus(.bad_request);
    try std.testing.expect(retired.bodyContains("UZ-REQ-001"));

    // Every allowlisted sort still orders: name ascending leads with a, name
    // descending leads with c.
    const asc = try (try h.get("/v1/api-keys?sort=key_name&limit=10").bearer(TOKEN_OPERATOR)).send();
    defer asc.deinit();
    try asc.expectStatus(.ok);
    const asc_parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, asc.body, .{});
    defer asc_parsed.deinit();
    const asc_items = asc_parsed.value.object.get("items").?.array;
    try std.testing.expectEqualStrings("sort-a-key", asc_items.items[0].object.get("key_name").?.string);

    const desc = try (try h.get("/v1/api-keys?sort=-key_name&limit=10").bearer(TOKEN_OPERATOR)).send();
    defer desc.deinit();
    try desc.expectStatus(.ok);
    const desc_parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, desc.body, .{});
    defer desc_parsed.deinit();
    const desc_items = desc_parsed.value.object.get("items").?.array;
    try std.testing.expectEqualStrings("sort-c-key", desc_items.items[0].object.get("key_name").?.string);

    const created_asc = try (try h.get("/v1/api-keys?sort=created_at&limit=10").bearer(TOKEN_OPERATOR)).send();
    defer created_asc.deinit();
    try created_asc.expectStatus(.ok);
    const created_desc = try (try h.get("/v1/api-keys?sort=-created_at&limit=10").bearer(TOKEN_OPERATOR)).send();
    defer created_desc.deinit();
    try created_desc.expectStatus(.ok);

    // Every way a cursor can be wrong answers 400, never a 500 and never a
    // silent walk of the wrong ordering. The cursor grammar is `{ts}:{id}` for
    // the created_at orderings and `s:{base64url(name)}:{id}` for the key_name
    // orderings, so these are hand-written rather than round-tripped.
    const REFUSED = [_][]const u8{
        // Timestamp form offered under a name ordering: resuming here would
        // silently paginate a different order.
        "/v1/api-keys?sort=key_name&starting_after=1744000000000:0195b4ba-8d3a-7f13-8abc-0000000d0001",
        // Name form offered under a created_at ordering — the mirror case.
        "/v1/api-keys?sort=-created_at&starting_after=s:c29ydC1hLWtleQ:0195b4ba-8d3a-7f13-8abc-0000000d0001",
        // Right form, but the id half is not a uuid: it seeks a ::uuid bind, so
        // an unguarded value would surface as a cast error's 500.
        "/v1/api-keys?sort=-created_at&starting_after=1744000000000:not-a-uuid",
        // Not a cursor at all.
        "/v1/api-keys?starting_after=garbage",
    };
    for (REFUSED) |path| {
        const refused = try (try h.get(path).bearer(TOKEN_OPERATOR)).send();
        defer refused.deinit();
        try refused.expectStatus(.bad_request);
        try std.testing.expect(refused.bodyContains("UZ-REQ-001"));
    }
    finalCleanup(h);
}

test "integration: test_api_keys_key_name_sort_pages_without_loss" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    // Seven keys, two sharing a name prefix — the composite (key_name, id)
    // boundary must not lose either of them at a page edge.
    const names = [_][]const u8{ "walk-a", "walk-b", "walk-dup", "walk-dup-2", "walk-e", "walk-f", "walk-g" };
    for (names) |name| try mintKey(h, name);

    var ids = try walkKeys(h, "key_name", 3);
    defer freeWalkedIds(&ids);
    try std.testing.expectEqual(@as(usize, 7), ids.items.len);
    for (ids.items, 0..) |a, i| {
        for (ids.items[i + 1 ..]) |b| try std.testing.expect(!std.mem.eql(u8, a, b));
    }
    finalCleanup(h);
}
