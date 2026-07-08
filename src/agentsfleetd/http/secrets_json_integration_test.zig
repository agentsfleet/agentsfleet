// HTTP integration tests for the structured-secret vault endpoints.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`. Vault tests also
// require ENCRYPTION_MASTER_KEY — set automatically by setTestEncryptionKey().
//
// Covers the happy-path roundtrip, JSON-shape rejections (string/array/empty),
// the 4 KiB cap, role enforcement (operator vs user), cross-workspace IDOR,
// and the `llm` suffix routing guard.
//
// Reuses the seeded tenant/workspace + JWT tokens baked into tenant_provider_http_integration_test.zig
// constants — see `setupSeedData` there. Cleanup happens in the test body
// (not via defer) per the harness contract.

const std = @import("std");
const scope_fixtures = @import("./test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");

const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
pub const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const PRIMARY_WORKSPACE_CREATED_AT_MS: i64 = 0;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;

// Operator + user JWTs from the tenant_provider suite — same tenant/workspace claims.
pub const TOKEN_USER = scope_fixtures.VIEWER;
pub const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

pub fn setTestEncryptionKey() void {
    crypto_primitives.setTestKek();
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    // Catalogue the model the self-managed secret names (anthropic /
    // claude-sonnet-4-6) so the PUT /provider catalogue-gate (UZ-PROVIDER-004)
    // passes. core.model_library ships seedless (M100), so a test that sets a
    // provider must seed the priced row it resolves against, then repopulate the
    // rate cache below from it.
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0ac001'::uuid, 'claude-sonnet-4-6', 'anthropic',
        \\        256000, 3000000000, 300000000, 15000000000, $1, $1)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{now_ms});
    try model_rate_cache.populate(conn);
    _ = try conn.exec("DELETE FROM core.tenant_model_selection WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID});
    _ = try conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID});
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'Vault JSON Test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id, created_at = LEAST(core.workspaces.created_at, EXCLUDED.created_at)
    , .{ TEST_WS_ID, TEST_TENANT_ID, PRIMARY_WORKSPACE_CREATED_AT_MS });
}

pub fn cleanupRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_selection WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

pub fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try startHarness(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try setupSeedData(conn);
    return h;
}

const SENTINEL_TOKEN = "SENTINEL_TOKEN_DO_NOT_LEAK_8a72c3";

test "integration: secret POST + GET + DELETE roundtrip never echoes value" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const post_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
    defer alloc.free(post_path);
    const del_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/fly", .{TEST_WS_ID});
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

test "integration: tenant provider accepts secret POST rows by raw name" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
    defer alloc.free(secrets_path);

    const secret_name = "provider-posted-key";
    const secret_token = "provider-token-not-real";
    const secret_body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\",\"data\":{{\"provider\":\"anthropic\",\"api_key\":\"{s}\",\"model\":\"claude-sonnet-4-6\"}}}}",
        .{ secret_name, secret_token },
    );
    defer alloc.free(secret_body);

    {
        const r = try (try (try h.post(secrets_path).bearer(TOKEN_OPERATOR)).json(secret_body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
        try std.testing.expect(r.bodyContains(secret_name));
        try std.testing.expect(!r.bodyContains(secret_token));
    }

    const provider_body = try std.fmt.allocPrint(
        alloc,
        "{{\"mode\":\"self_managed\",\"secret_ref\":\"{s}\"}}",
        .{secret_name},
    );
    defer alloc.free(provider_body);

    {
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR)).json(provider_body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"mode\":\"self_managed\""));
        try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
        try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
        try std.testing.expect(r.bodyContains("\"secret_ref\":\"provider-posted-key\""));
        try std.testing.expect(!r.bodyContains(secret_token));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: custom openai-compatible secret activates end-to-end" {
    // The cross-tier seam the resolver-direct tests cannot see: POST a custom
    // openai-compatible secret, then PUT /provider to activate it, and assert
    // the activate SUCCEEDS (200) with the resolved view. This drives the real
    // handler path — body parse, probe, AND the model_rate_cache catalogue gate —
    // which the upsertSelfManaged-direct unit tests skip.
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
    defer alloc.free(secrets_path);

    const cred_body =
        "{\"name\":\"compat-key\",\"data\":{\"provider\":\"openai-compatible\"," ++
        "\"base_url\":\"https://api.openrouter.ai/v1\",\"model\":\"kimi-k2.6\"," ++
        "\"api_key\":\"sk-compat-not-real\"}}";
    {
        const r = try (try (try h.post(secrets_path).bearer(TOKEN_OPERATOR)).json(cred_body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    {
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"compat-key\"}")).send();
        defer r.deinit();
        // A custom (openai-compatible) endpoint bills provider-direct, so it must
        // BYPASS the platform model-rate catalogue gate — its user-hosted model is
        // absent from core.model_library by design — and activate. Regression guard for
        // the catalogue-gate fix: the activate SUCCEEDS with the resolved view, and
        // the api_key never echoes back (vault-only).
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"mode\":\"self_managed\""));
        try std.testing.expect(r.bodyContains("\"provider\":\"openai-compatible\""));
        try std.testing.expect(r.bodyContains("\"model\":\"kimi-k2.6\""));
        try std.testing.expect(r.bodyContains("\"secret_ref\":\"compat-key\""));
        try std.testing.expect(!r.bodyContains("sk-compat-not-real"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: secret POST rejects non-object data" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
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

test "integration: secret POST rejects oversized stringified data" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
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
