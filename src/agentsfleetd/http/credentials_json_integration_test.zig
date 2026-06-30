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
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const PRIMARY_WORKSPACE_CREATED_AT_MS: i64 = 0;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;

// Operator + user JWTs from the tenant_provider suite — same tenant/workspace claims.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLnRlc3QuYWdlbnRzZmxlZXQubmV0IiwiYXVkIjoiaHR0cHM6Ly9hcGkuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwLCJzY29wZXMiOiJmbGVldDphZG1pbiBjcmVkZW50aWFsOndyaXRlIGFwaWtleTphZG1pbiBmbGVldGtleTp3cml0ZSBncmFudDp3cml0ZSBjb25uZWN0b3I6d3JpdGUgYmlsbGluZzpyZWFkIGFwcHJvdmFsOnJlc29sdmUgd29ya3NwYWNlOmFkbWluIHRlbXBsYXRlOndyaXRlIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIn19.clzrJQSbL5tON0PQQwuJYCRDJVDHiebt40X0wYNsN93A6KlNcLO2I_zREIXn2aUI8HAN0WaVJKGHuh1RXuQ-4Fw4wUS7UFIlrY_4DWKkTg6WCbAXxhwe90ScOn9Q5oXUfDLTbpMGw1sFgLe67qy2QPdyH_yephKyjArBnwJQqMbXtb-uKXN66lcrgHlR-KoBGzqkDHyc5bVy9CPKiLgbzZQac1mug53gc8zOZeAFlfgTXTWdSn65f37Cd-vmbGngrhY6sH2oZcUGOlXPiZtyw7jgWyp6tL9gLiDEwwLbQFkUqVvUjjhmkY8-LG7nna-ratPpt5UK3r7WB4bjREbsyQ";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLnRlc3QuYWdlbnRzZmxlZXQubmV0IiwiYXVkIjoiaHR0cHM6Ly9hcGkuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwLCJzY29wZXMiOiJmbGVldDphZG1pbiBjcmVkZW50aWFsOndyaXRlIGFwaWtleTphZG1pbiBmbGVldGtleTp3cml0ZSBncmFudDp3cml0ZSBjb25uZWN0b3I6d3JpdGUgYmlsbGluZzpyZWFkIGFwcHJvdmFsOnJlc29sdmUgd29ya3NwYWNlOmFkbWluIHRlbXBsYXRlOndyaXRlIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIn19.clzrJQSbL5tON0PQQwuJYCRDJVDHiebt40X0wYNsN93A6KlNcLO2I_zREIXn2aUI8HAN0WaVJKGHuh1RXuQ-4Fw4wUS7UFIlrY_4DWKkTg6WCbAXxhwe90ScOn9Q5oXUfDLTbpMGw1sFgLe67qy2QPdyH_yephKyjArBnwJQqMbXtb-uKXN66lcrgHlR-KoBGzqkDHyc5bVy9CPKiLgbzZQac1mug53gc8zOZeAFlfgTXTWdSn65f37Cd-vmbGngrhY6sH2oZcUGOlXPiZtyw7jgWyp6tL9gLiDEwwLbQFkUqVvUjjhmkY8-LG7nna-ratPpt5UK3r7WB4bjREbsyQ";

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
    crypto_primitives.setTestKek();
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    // Catalogue the model the self-managed credential names (anthropic /
    // claude-sonnet-4-6) so the PUT /provider catalogue-gate (UZ-PROVIDER-004)
    // passes. core.model_caps ships seedless (M100), so a test that sets a
    // provider must seed the priced row it resolves against, then repopulate the
    // rate cache below from it.
    _ = try conn.exec(
        \\INSERT INTO core.model_caps
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0ac001'::uuid, 'claude-sonnet-4-6', 'anthropic',
        \\        256000, 3000000000, 300000000, 15000000000, $1, $1)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{now_ms});
    try model_rate_cache.populate(conn);
    _ = try conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID});
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

fn cleanupRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
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

test "integration: tenant provider accepts credential POST rows by raw name" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const credentials_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(credentials_path);

    const credential_name = "provider-posted-key";
    const credential_token = "provider-token-not-real";
    const credential_body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\",\"data\":{{\"provider\":\"anthropic\",\"api_key\":\"{s}\",\"model\":\"claude-sonnet-4-6\"}}}}",
        .{ credential_name, credential_token },
    );
    defer alloc.free(credential_body);

    {
        const r = try (try (try h.post(credentials_path).bearer(TOKEN_OPERATOR)).json(credential_body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
        try std.testing.expect(r.bodyContains(credential_name));
        try std.testing.expect(!r.bodyContains(credential_token));
    }

    const provider_body = try std.fmt.allocPrint(
        alloc,
        "{{\"mode\":\"self_managed\",\"credential_ref\":\"{s}\"}}",
        .{credential_name},
    );
    defer alloc.free(provider_body);

    {
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR)).json(provider_body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"mode\":\"self_managed\""));
        try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
        try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
        try std.testing.expect(r.bodyContains("\"credential_ref\":\"provider-posted-key\""));
        try std.testing.expect(!r.bodyContains(credential_token));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: custom openai-compatible credential activates end-to-end" {
    // The cross-tier seam the resolver-direct tests cannot see: POST a custom
    // openai-compatible credential, then PUT /provider to activate it, and assert
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

    const credentials_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(credentials_path);

    const cred_body =
        "{\"name\":\"compat-key\",\"data\":{\"provider\":\"openai-compatible\"," ++
        "\"base_url\":\"https://api.openrouter.ai/v1\",\"model\":\"kimi-k2.6\"," ++
        "\"api_key\":\"sk-compat-not-real\"}}";
    {
        const r = try (try (try h.post(credentials_path).bearer(TOKEN_OPERATOR)).json(cred_body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    {
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"credential_ref\":\"compat-key\"}")).send();
        defer r.deinit();
        // A custom (openai-compatible) endpoint bills provider-direct, so it must
        // BYPASS the platform model-rate catalogue gate — its user-hosted model is
        // absent from core.model_caps by design — and activate. Regression guard for
        // the catalogue-gate fix: the activate SUCCEEDS with the resolved view, and
        // the api_key never echoes back (vault-only).
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"mode\":\"self_managed\""));
        try std.testing.expect(r.bodyContains("\"provider\":\"openai-compatible\""));
        try std.testing.expect(r.bodyContains("\"model\":\"kimi-k2.6\""));
        try std.testing.expect(r.bodyContains("\"credential_ref\":\"compat-key\""));
        try std.testing.expect(!r.bodyContains("sk-compat-not-real"));
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

// ── §1: list metadata projection ────────────────────────────────────────────

test "integration: list projects kind + non-secret metadata, never the api_key" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(creds_path);

    // Sentinels that must never reappear in the list response (any kind).
    const PROVIDER_KEY_SECRET = "sk-ant-PROVIDER-DO-NOT-LEAK-7f1a";
    const ENDPOINT_SECRET = "sk-compat-DO-NOT-LEAK-9c2b";
    const SECRET_TOKEN = "stripe-DO-NOT-LEAK-3e4d";

    // One of each kind: named provider key, custom openai-compatible endpoint,
    // opaque secret (no `provider` field).
    const bodies = [_][]const u8{
        "{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ PROVIDER_KEY_SECRET ++ "\",\"model\":\"claude-sonnet-4-6\"}}",
        "{\"name\":\"vllm-gw\",\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"model\":\"kimi-k2.6\",\"api_key\":\"" ++ ENDPOINT_SECRET ++ "\"}}",
        "{\"name\":\"STRIPE_API_KEY\",\"data\":{\"api_token\":\"" ++ SECRET_TOKEN ++ "\"}}",
    };
    for (bodies) |b| {
        const r = try (try (try h.post(creds_path).bearer(TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const r = try (try h.get(creds_path).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // Each credential is classified by the server.
    try std.testing.expect(r.bodyContains("\"kind\":\"provider_key\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"custom_endpoint\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"custom_secret\""));
    // Non-secret descriptors are surfaced…
    try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
    try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
    try std.testing.expect(r.bodyContains("\"provider\":\"openai-compatible\""));
    try std.testing.expect(r.bodyContains("\"base_url\":\"https://gw.example.com/v1\""));
    // …but the api_key — for every kind — is never read into the response.
    try std.testing.expect(!r.bodyContains(PROVIDER_KEY_SECRET));
    try std.testing.expect(!r.bodyContains(ENDPOINT_SECRET));
    try std.testing.expect(!r.bodyContains(SECRET_TOKEN));
    try std.testing.expect(!r.bodyContains("api_key"));
    try std.testing.expect(!r.bodyContains("api_token"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: GET list requires operator role" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(creds_path);

    // No bearer → 401; user role → 403 (the projection runs only past the gate).
    {
        const r = try h.get(creds_path).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    {
        const r = try (try h.get(creds_path).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

// ── §2: key-only credential rotate (PATCH) ──────────────────────────────────

test "integration: rotate replaces only the api_key, preserving provider/model/base_url" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(creds_path);
    const named_item = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/anthropic-prod", .{TEST_WS_ID});
    defer alloc.free(named_item);
    const endpoint_item = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/vllm-gw", .{TEST_WS_ID});
    defer alloc.free(endpoint_item);

    const OLD_KEY = "sk-OLD-DO-NOT-LEAK-aa11";
    const NEW_KEY = "sk-NEW-DO-NOT-LEAK-bb22";

    const seed_bodies = [_][]const u8{
        "{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ OLD_KEY ++ "\",\"model\":\"claude-sonnet-4-6\"}}",
        "{\"name\":\"vllm-gw\",\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"model\":\"kimi-k2.6\",\"api_key\":\"" ++ OLD_KEY ++ "\"}}",
    };
    for (seed_bodies) |b| {
        const r = try (try (try h.post(creds_path).bearer(TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    // Rotate both; the body echoes only the name, never the key.
    const rotate_body = "{\"api_key\":\"" ++ NEW_KEY ++ "\"}";
    {
        const r = try (try (try h.patch(named_item).bearer(TOKEN_OPERATOR)).json(rotate_body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"name\":\"anthropic-prod\""));
        try std.testing.expect(!r.bodyContains(NEW_KEY));
    }
    {
        const r = try (try (try h.patch(endpoint_item).bearer(TOKEN_OPERATOR)).json(rotate_body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    // Non-secret fields survive the rotate; neither old nor new key is exposed.
    const r = try (try h.get(creds_path).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
    try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"provider_key\""));
    try std.testing.expect(r.bodyContains("\"base_url\":\"https://gw.example.com/v1\""));
    try std.testing.expect(r.bodyContains("\"model\":\"kimi-k2.6\""));
    try std.testing.expect(!r.bodyContains(OLD_KEY));
    try std.testing.expect(!r.bodyContains(NEW_KEY));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: rotate a missing credential returns typed 404" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/does-not-exist", .{TEST_WS_ID});
    defer alloc.free(path);

    const r = try (try (try h.patch(path).bearer(TOKEN_OPERATOR)).json("{\"api_key\":\"sk-whatever\"}")).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
    try std.testing.expect(r.bodyContains(error_codes.ERR_CREDENTIAL_NOT_FOUND));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: rotate rejects an empty or oversized key without leaking it" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials", .{TEST_WS_ID});
    defer alloc.free(creds_path);
    const item_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/anthropic-prod", .{TEST_WS_ID});
    defer alloc.free(item_path);

    {
        const r = try (try (try h.post(creds_path).bearer(TOKEN_OPERATOR))
            .json("{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-seed\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    // Empty key → 400, typed invalid-request.
    {
        const r = try (try (try h.patch(item_path).bearer(TOKEN_OPERATOR)).json("{\"api_key\":\"\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_INVALID_REQUEST));
    }

    // Oversized key (5 KiB → re-stringified body exceeds the 4 KiB cap) → 400,
    // and the key bytes never echo back in the error envelope.
    {
        const filler = try alloc.alloc(u8, 5 * 1024);
        defer alloc.free(filler);
        @memset(filler, 'k');
        const body = try std.fmt.allocPrint(alloc, "{{\"api_key\":\"{s}\"}}", .{filler});
        defer alloc.free(body);
        const r = try (try (try h.patch(item_path).bearer(TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_VAULT_DATA_TOO_LARGE));
        try std.testing.expect(!r.bodyContains(filler));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: cross-workspace DELETE is rejected (IDOR guard)" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // TOKEN_OPERATOR's JWT claim binds it to TEST_WS_ID. Issue a DELETE
    // against a *different* workspace UUID — workspace_guards.enforce
    // must reject (4xx), never 204. Without this check, a workspace-A
    // operator could nuke a workspace-B credential just by URL editing.
    const other_ws = "0195b4ba-8d3a-7f13-8abc-deadbeef0001";
    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/fly", .{other_ws});
    defer alloc.free(path);

    const r = try (try h.delete(path).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    // Concrete code is 403 (Workspace access denied), but the invariant is
    // "not 204" — any 4xx is an acceptable rejection. Anchoring to "≥ 400"
    // keeps the test resilient if the guard later returns 404 for IDOR safety.
    try std.testing.expect(r.status >= 400);
    try std.testing.expect(r.status != 204);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}
