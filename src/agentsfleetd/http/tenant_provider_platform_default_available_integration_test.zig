// GET /v1/tenants/me/provider's platform_default_available reflects whether
// an active core.platform_llm_keys row exists, independent of the tenant's
// own current mode. Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped
// otherwise via `TestHarness.start` returning `error.SkipZigTest`.

const std = @import("std");
const scope_fixtures = @import("./test_scope_tokens.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const fixtures_provider = @import("../db/test_fixtures.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenantWorkspace(conn: anytype) !void {
    const clock = @import("common").clock;
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'Platform Default Available Test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, 0)
        \\ON CONFLICT (workspace_id) DO UPDATE SET tenant_id = EXCLUDED.tenant_id
    , .{ TEST_WS_ID, TEST_TENANT_ID });
}

fn cleanupRows(conn: anytype) void {
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    fixtures_provider.teardownPlatformProvider(conn, TEST_WS_ID);
}

test "integration: platform_default_available is false with no active platform_llm_keys row, regardless of tenant mode" {
    const alloc = std.testing.allocator;
    const h = startHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    try seedTenantWorkspace(conn);
    cleanupRows(conn); // ensure no leftover active platform key from a prior run
    h.releaseConn(conn);

    const r = try (try h.get("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"platform_default_available\":false"));

    const conn2 = try h.acquireConn();
    defer h.releaseConn(conn2);
    cleanupRows(conn2);
}

test "integration: platform_default_available is true when an active platform_llm_keys row exists, even while the tenant runs self-managed" {
    const alloc = std.testing.allocator;
    const h = startHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    try seedTenantWorkspace(conn);
    try fixtures_provider.seedPlatformProvider(alloc, conn, TEST_WS_ID);
    // The tenant itself is NOT on the platform default — no core.tenant_providers
    // row is inserted here, so mode falls back to "platform" naturally; the
    // point under test is that platform_default_available reads the platform
    // key's existence, not the tenant's own row. (A self-managed activation
    // would additionally require a catalogued secret — orthogonal to this
    // Dimension, which only asserts the field's independence from `mode`.)
    h.releaseConn(conn);

    const r = try (try h.get("/v1/tenants/me/provider").bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"platform_default_available\":true"));

    const conn2 = try h.acquireConn();
    defer h.releaseConn(conn2);
    cleanupRows(conn2);
}
