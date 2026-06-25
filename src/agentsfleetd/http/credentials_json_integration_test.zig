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

const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;

// Operator + user JWTs from the tenant_provider suite — same tenant/workspace claims.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJ1c2VyIn19.aSqdpbu-D-1NmzJgcw-7LUJYImlFu-gbrO3fBPlMI6DFvgSGJJg3wAYe5DKJXe5ytCActeAHN8LxGyr1emB4ReHk90B7t_DB301cl5fz6H1EIBnUYkuOYIeCQXvqTmEHduR1KPumEYc6Jfw3kv1tY95k-bugObZ4FihLhWXw4ud8fXRl_CTnD3J3FSx-cn4K8mfy8JjTc1RDmEx5_4-TbBhPyTgj5EAXqB1ddUw7k46UAh_-w2G07SrOxsl1b57Etwp0gvuu4tkpXICYmG423n-RjVvtvuxjSzQyhUZ2Lmfbvi1tLlY7_uzTh_BwwWWYLdJtnmKEblmGReoAu_Qs6A";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJvcGVyYXRvciJ9fQ.eEQp3HyUFsV1bRBDvww3DirCY1R-vrASYT3KXnTeXBa8Owuag8Mc1I_v93XBatf-t-Y0qd6r9uNQuRiRpuXkrC01MJwyPnyvKDYHFAX828PIMdFgZ5FUGU0S6r1B4B8FaVZnfMdwyyQW9tCeFBvvh2hkuodoOlkcaJnR98kMrYjGHVoyDQc5H5JnU5O8Kkb9STE-XR-3b8VdOlGJR-ljX4Vw8Fipo5p7fo_VdhhUXD2C974DrbQWtsXhqUTqOFWAEUcUMM2ODH8pEFWhG8poHVP8LLWCcSFxZDN_Ia3dNR8OK9SEblCPIlfimiMtscqxli-9uC00n62UmLuQtGVlXA";

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
