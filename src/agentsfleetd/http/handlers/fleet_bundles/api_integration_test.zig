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
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

const EMPTY_REASONS_ID = "catalog-empty-reasons";
/// A DRAFT entry: stored, but never published. The bundles list must not carry it.
const DRAFT_PROBE_ID = "draft-visibility-probe";
const REVIEWER_ID = "github-pr-reviewer";
const REVIEWER_REASON = "review your pull requests and post review comments";

/// Seed a catalog row directly. No migration seeds the catalog any more (M128
/// Invariant 5), so a test that needs an entry creates it — which is also the
/// honest fixture: it states exactly the row shape the assertion depends on.
fn seedEntry(
    conn: *pg.Conn,
    id: []const u8,
    visibility: []const u8,
    credentials: []const u8,
    reasons: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_library
        \\    (id, name, description, source_repo, source_path, source_ref,
        \\     required_credentials, required_credentials_reasons, required_tools,
        \\     network_hosts, visibility, content_hash, skill_markdown,
        \\     created_at, updated_at)
        \\VALUES ($1, $1, 'Seeded by the test fixture.', 'agentsfleet/' || $1, '', 'main',
        \\        $3::jsonb, $4::jsonb, '[]'::jsonb, '[]'::jsonb, $2,
        \\        'deadbeef', '# skill', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ id, visibility, credentials, reasons });
}

test "integration: the bundles list carries published catalog entries" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // A published entry carrying curated per-credential copy...
    try seedEntry(conn, REVIEWER_ID, "public", "[\"github\"]", "{\"github\":\"" ++ REVIEWER_REASON ++ "\"}");
    // ...and one with NO reasons, to exercise the empty-map round-trip.
    try seedEntry(conn, EMPTY_REASONS_ID, "public", "[]", "{}");

    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains("\"items\""));
    try std.testing.expect(res.bodyContains(REVIEWER_ID));
    try std.testing.expect(res.bodyContains("\"required_credentials\""));
    try std.testing.expect(res.bodyContains("[\"github\"]"));
    try std.testing.expect(res.bodyContains("\"required_credentials_reasons\""));
    try std.testing.expect(res.bodyContains(REVIEWER_REASON));
    try std.testing.expect(res.bodyContains(EMPTY_REASONS_ID));
    try std.testing.expect(res.bodyContains("\"required_credentials_reasons\":{}"));
}

test "integration: the bundles list hides an unpublished entry" {
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
    try seedEntry(conn, DRAFT_PROBE_ID, "draft", "[]", "{}");
    try seedEntry(conn, REVIEWER_ID, "public", "[]", "{}");

    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(!res.bodyContains(DRAFT_PROBE_ID)); // a draft never reaches a tenant
    try std.testing.expect(res.bodyContains(REVIEWER_ID)); // public still shown

}
