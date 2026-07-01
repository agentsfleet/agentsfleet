//! Integration coverage for the platform template catalog gallery
//! (GET /v1/fleets/bundles → list.zig). The per-workspace bundle import/detail
//! endpoints and bundle_id install were removed in M103 §4 — install now flows
//! through the two template tiers (see handlers/templates/*).

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_USER = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn resetAndSeed(conn: *pg.Conn) !void {
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

const EMPTY_REASONS_ID = "catalog-empty-reasons";
const PRIVATE_PROBE_ID = "private-visibility-probe";

test "integration: template catalog lists seeded first-party templates from the table" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Seed one PUBLIC template with NO per-credential reasons, to exercise the
    // empty-map round-trip. Removed at the end so the curated set is left intact.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_bundle_templates
        \\    (id, name, description, source_repo, source_path, source_ref,
        \\     required_credentials, required_credentials_reasons, required_tools, network_hosts, visibility,
        \\     created_at, updated_at)
        \\VALUES
        \\    ($1, 'Empty reasons', 'Public row with no per-credential reasons.',
        \\     'agentsfleet/catalog-empty-reasons', '', 'main',
        \\     '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 'public', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{EMPTY_REASONS_ID});

    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains("\"items\""));
    try std.testing.expect(res.bodyContains("\"github-pr-reviewer\""));
    try std.testing.expect(res.bodyContains("\"security-reviewer\""));
    try std.testing.expect(res.bodyContains("\"required_credentials\""));
    try std.testing.expect(res.bodyContains("[\"github\"]"));
    try std.testing.expect(res.bodyContains("\"required_credentials_reasons\""));
    try std.testing.expect(res.bodyContains("review your pull requests and post review comments"));
    try std.testing.expect(res.bodyContains(EMPTY_REASONS_ID));
    try std.testing.expect(res.bodyContains("\"required_credentials_reasons\":{}"));

    _ = try conn.exec("DELETE FROM core.fleet_bundle_templates WHERE id = $1", .{EMPTY_REASONS_ID});
}

test "integration: catalog hides non-public templates (visibility filter)" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Seed a private-visibility row directly — the catalog filters
    // `WHERE visibility = 'public'`, so this probe must NOT surface.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_bundle_templates
        \\    (id, name, description, source_repo, source_path, source_ref,
        \\     required_credentials, required_credentials_reasons, required_tools, network_hosts, visibility,
        \\     created_at, updated_at)
        \\VALUES
        \\    ($1, 'Private probe', 'Hidden from the gallery.',
        \\     'agentsfleet/private-visibility-probe', '', 'main',
        \\     '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 'private', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{PRIVATE_PROBE_ID});

    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(!res.bodyContains(PRIVATE_PROBE_ID)); // private hidden
    try std.testing.expect(res.bodyContains("\"github-pr-reviewer\"")); // public still shown

    _ = try conn.exec("DELETE FROM core.fleet_bundle_templates WHERE id = $1", .{PRIVATE_PROBE_ID});
}
