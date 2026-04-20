// Integration tests for /v1/api-keys (M28_002 §3, §4).
//
// Covers:
//   - POST creates: 201, zmb_t_ prefix, SHA-256 hex persisted in core.api_keys.
//   - Duplicate key_name within a tenant: 409 UZ-APIKEY-005.
//   - Round-trip auth: a minted zmb_t_ key authenticates a subsequent GET.
//   - PATCH {active:false} revokes; the same key can no longer authenticate.
//   - DELETE on an active key is 409; DELETE on a revoked key is 204.
//   - Tenant isolation: GET as tenant A does not return tenant B's rows.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see
// docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness".
//
// The operator JWT and JWKS mirror cross_workspace_idor_test.zig so any future
// key rotation in that test updates both at once — do NOT regenerate here.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../auth/middleware/mod.zig");
const api_key_lookup = @import("../../cmd/api_key_lookup.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc01";
const FOREIGN_KEY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc02";

const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

// Real DB-backed api-key lookup. The ctx must outlive the middleware chain, so
// we park it at module scope — `zig build test` runs tests sequentially in a
// single process, so reassigning across tests is safe (each reassignment
// happens after the previous harness's deinit sets the chain pointer stale).
// If the test runner ever parallelizes, move into TestHarness as an extension.
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
    const now = std.time.milliTimestamp();
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
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/api-keys', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
}

fn cleanupApiKeys(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.api_keys WHERE tenant_id IN ($1::uuid, $2::uuid)", .{ TEST_TENANT_ID, OTHER_TENANT_ID }) catch {};
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

test "integration: POST /v1/api-keys returns 201 with zmb_t_ key and persists SHA-256 hash" {
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
    try std.testing.expect(std.mem.startsWith(u8, raw_key, "zmb_t_"));
    try std.testing.expectEqual(@as(usize, 70), raw_key.len);

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

test "integration: minted zmb_t_ key authenticates GET, revoked by PATCH {active:false}" {
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

    // Authenticate GET using the minted raw key (Bearer <zmb_t_...>).
    const list_before = try (try h.get("/v1/api-keys").bearer(raw_key)).send();
    defer list_before.deinit();
    try list_before.expectStatus(.ok);
    try std.testing.expect(!list_before.bodyContains("key_hash"));

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

test "integration: DELETE active key → 409, revoked key → 204" {
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
            \\INSERT INTO core.api_keys (id, tenant_id, key_name, key_hash, created_by, active)
            \\VALUES ($1::uuid, $2::uuid, 'other-tenant-key', 'deadbeef' , 'user_other', TRUE)
        , .{ FOREIGN_KEY_ID, OTHER_TENANT_ID });
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
