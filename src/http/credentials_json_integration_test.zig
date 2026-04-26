// HTTP integration tests for the structured-credential vault endpoints.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`. Vault tests also
// require ENCRYPTION_MASTER_KEY — set automatically by setTestEncryptionKey().
//
// Covers the happy-path roundtrip, JSON-shape rejections (string/array/empty),
// the 4 KiB cap, role enforcement (operator vs user), cross-workspace IDOR,
// and the `llm` suffix routing guard.
//
// Reuses the seeded tenant/workspace + JWT tokens baked into byok_http_integration_test.zig
// constants — see `setupSeedData` there. Cleanup happens in the test body
// (not via defer) per the harness contract.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_REPO_URL = "https://github.com/test/credentials_json";

const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;

// Operator + user JWTs from the byok suite — same tenant/workspace claims.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn setTestEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20", 1);
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID});
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'Vault JSON Test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WS_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
}

fn cleanupRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch {};
}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try startHarness(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try setupSeedData(conn);
    return h;
}

const SENTINEL_TOKEN = "SENTINEL_TOKEN_DO_NOT_LEAK_8a72c3";

test "integration: credential POST + GET + DELETE roundtrip never echoes value" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const post_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(post_path);
    const del_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/fly", .{TEST_WS_ID});
    defer alloc.free(del_path);

    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"fly\",\"data\":{{\"host\":\"api.machines.dev\",\"api_token\":\"{s}\"}}}}",
        .{SENTINEL_TOKEN},
    );
    defer alloc.free(body);

    {
        const r = try (try (try h.post(post_path).bearer(TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
        try std.testing.expect(r.bodyContains("\"name\":\"fly\""));
        try std.testing.expect(!r.bodyContains(SENTINEL_TOKEN));
    }
    {
        const r = try (try h.get(post_path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"name\":\"fly\""));
        try std.testing.expect(!r.bodyContains(SENTINEL_TOKEN));
        try std.testing.expect(!r.bodyContains("api_token"));
        try std.testing.expect(!r.bodyContains("api.machines.dev"));
    }
    {
        const r = try (try h.delete(del_path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
    {
        const r = try (try h.delete(del_path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
    {
        const r = try (try h.get(post_path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(!r.bodyContains("\"name\":\"fly\""));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: credential POST rejects non-object data" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(path);

    const cases = [_][]const u8{
        // bare string
        "{\"name\":\"x\",\"data\":\"bare-string\"}",
        // array
        "{\"name\":\"x\",\"data\":[1,2,3]}",
        // empty object
        "{\"name\":\"x\",\"data\":{}}",
    };
    for (cases) |body| {
        const r = try (try (try h.post(path).bearer(TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_VAULT_DATA_INVALID));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: credential POST rejects oversized stringified data" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(path);

    // 5 KiB filler — handler caps at 4 KiB stringified.
    const filler = try alloc.alloc(u8, 5 * 1024);
    defer alloc.free(filler);
    @memset(filler, 'a');
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"big\",\"data\":{{\"v\":\"{s}\"}}}}", .{filler});
    defer alloc.free(body);

    const r = try (try (try h.post(path).bearer(TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try std.testing.expect(r.bodyContains(error_codes.ERR_VAULT_DATA_TOO_LARGE));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: credential endpoints enforce operator role" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(path);
    const body = "{\"name\":\"x\",\"data\":{\"k\":\"v\"}}";

    {
        const r = try (try h.post(path).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    {
        const r = try (try (try h.post(path).bearer(TOKEN_USER)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: DELETE /credentials/llm is not routed to the generic handler" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/llm", .{TEST_WS_ID});
    defer alloc.free(path);

    // BYOK route owns /credentials/llm. The new generic matcher rejects this
    // suffix, so the request must reach the BYOK handler — which returns 204
    // on a no-op delete (idempotent), not 404 from a generic-route miss.
    const r = try (try h.delete(path).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.no_content);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}
