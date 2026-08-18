// The refusal halves of GET/PUT/DELETE /v1/tenants/me/provider, plus the two
// verbs that write the explicit platform-default row. The sibling suites drive
// the self_managed success path and `platform_default_available`; every arm the
// handler takes when the caller is wrong was dark — a caller with no tenant
// claim, a body that is absent or not JSON, a mode the handler does not know,
// and each `ResolveError` the credential probe can raise.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.

const std = @import("std");
const pg = @import("pg");
const scope_fixtures = @import("./test_scope_tokens.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const ec = @import("../errors/error_registry.zig");
const fixtures_provider = @import("../db/test_fixtures.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// The shared seeded tenant/workspace the provider suites already use — same
// literals, same rows (see tenant_provider_platform_default_available_integration_test.zig).
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

const PROVIDER_PATH = "/v1/tenants/me/provider";

const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;
// Carries `secret:write` but an empty `metadata` claim, so it clears the route's
// scope gate and reaches the handler with `principal.tenant_id == null` — the
// only way to drive the tenant-context refusal through the real middleware.
const TOKEN_NO_TENANT = scope_fixtures.NO_TENANT;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedTenantWorkspace(conn: *pg.Conn) !void {
    const clock = @import("common").clock;
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'Provider Error Arms Test', $2, $2)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    // created_at 0 keeps this the tenant's primary workspace, which the
    // credential probe resolves against.
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, 0)
        \\ON CONFLICT (id) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id, created_at = EXCLUDED.created_at
    , .{ TEST_WS_ID, TEST_TENANT_ID });
}

/// Explicit, not deferred: deferred cleanup leaks pool connections at
/// `pool.deinit()`, so every test calls this in its body.
fn cleanupRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_selection WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    fixtures_provider.teardownPlatformProvider(conn, TEST_WS_ID);
}

fn seededHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try startHarness(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenantWorkspace(conn);
    cleanupRows(conn);
    return h;
}

fn postSecret(h: *TestHarness, alloc: std.mem.Allocator, body: []const u8) !void {
    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{TEST_WS_ID});
    defer alloc.free(path);
    const r = try (try (try h.post(path).bearer(TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
}

test "integration: every verb on /provider refuses a caller carrying no tenant claim" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // GET, PUT and DELETE each read the tenant claim before touching the pool,
    // so all three must refuse identically rather than 500 on a null tenant.
    {
        const r = try (try h.get(PROVIDER_PATH).bearer(TOKEN_NO_TENANT)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    {
        const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_NO_TENANT))
            .json("{\"mode\":\"platform\"}")).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    {
        const r = try (try h.delete(PROVIDER_PATH).bearer(TOKEN_NO_TENANT)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: PUT /provider refuses an absent body, a non-JSON body, and an unknown mode" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // An empty body — the handler must not reach the JSON parser. It is sent as
    // a zero-length body rather than a bodiless request because std's client
    // asserts `!method.requestHasBody()` in `sendBodilessUnflushed`, so a
    // bodiless PUT panics in the client and never reaches the server at all.
    {
        const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR)).json("")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }
    // A body that is not JSON at all.
    {
        const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
            .json("not-json-at-all")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }
    // Well-formed JSON naming a mode the handler does not implement. This is
    // the arm that falls off the end of both mode comparisons.
    {
        const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
            .json("{\"mode\":\"byo_gpu\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: PUT mode=platform writes the explicit platform row and returns the resolved view" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures_provider.seedPlatformProvider(alloc, conn, TEST_WS_ID);
    }

    const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
        .json("{\"mode\":\"platform\"}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // The response is the re-read view, not an echo of the request — an
    // explicit platform row reads back as platform mode with no secret_ref.
    try std.testing.expect(r.bodyContains("\"mode\":\"platform\""));
    try std.testing.expect(r.bodyContains("\"secret_ref\":null"));
    try std.testing.expect(r.bodyContains("\"platform_default_available\":true"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: DELETE /provider is the explicit reset — same written row as PUT mode=platform" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures_provider.seedPlatformProvider(alloc, conn, TEST_WS_ID);
    }

    const r = try (try h.delete(PROVIDER_PATH).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"mode\":\"platform\""));
    try std.testing.expect(r.bodyContains("\"secret_ref\":null"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: mode=self_managed without a secret_ref is refused before any vault read" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
        .json("{\"mode\":\"self_managed\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_PROVIDER_SECRET_REF_REQUIRED);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: mode=self_managed naming a credential the vault does not hold is refused" {
    fixtures_provider.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
        .json("{\"mode\":\"self_managed\",\"secret_ref\":\"no-such-credential\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_PROVIDER_SECRET_NOT_FOUND);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: mode=self_managed on a credential whose JSON names no provider is refused" {
    fixtures_provider.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // A perfectly valid secret for any other consumer — it just carries none of
    // the fields the provider probe requires.
    try postSecret(h, alloc, "{\"name\":\"shapeless-key\",\"data\":{\"api_key\":\"sk-not-real\"}}");

    const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
        .json("{\"mode\":\"self_managed\",\"secret_ref\":\"shapeless-key\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_PROVIDER_SECRET_DATA_MALFORMED);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}

test "integration: mode=self_managed on an openai-compatible credential with no base_url is refused" {
    fixtures_provider.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // openai-compatible is the one provider whose endpoint the tenant supplies,
    // so an absent base_url leaves the runner with nowhere to send the call.
    try postSecret(h, alloc, "{\"name\":\"endpointless-key\",\"data\":{\"provider\":\"openai-compatible\"," ++
        "\"model\":\"kimi-k2.6\",\"api_key\":\"sk-not-real\"}}");

    const r = try (try (try h.put(PROVIDER_PATH).bearer(TOKEN_OPERATOR))
        .json("{\"mode\":\"self_managed\",\"secret_ref\":\"endpointless-key\"}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_PROVIDER_BASE_URL_INVALID);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupRows(conn);
}
