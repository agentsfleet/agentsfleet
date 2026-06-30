//! Integration tests for POST /v1/workspaces.
//!
//! Covers the post-rename behaviour:
//!   - empty body `{}` succeeds; server picks a Heroku-style name
//!   - explicit `{"name": "..."}` succeeds and stores that exact name
//!   - two consecutive empty POSTs return distinct names (proves the
//!     auto-name path wires through real generator state, not a stub)
//!   - duplicate explicit name within the same tenant is rejected
//!   - missing tenant principal returns 401
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Reuse the tenant + signed token from tenant_workspaces_integration_test —
// same TEST_JWKS, so the rsa256 signature validates.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
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

fn seedTenant(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'CreateWsTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
}

/// Pull `"name":"…"` out of the create response body. Returns null when the
/// field is absent — the contract under test is that POST /v1/workspaces
/// always returns it, so a null here is a real test failure caller-side.
fn extractName(alloc: std.mem.Allocator, body: []const u8) !?[]u8 {
    const key = "\"name\":\"";
    const start = std.mem.indexOf(u8, body, key) orelse return null;
    const after = start + key.len;
    const end_rel = std.mem.indexOfScalar(u8, body[after..], '"') orelse return null;
    return try alloc.dupe(u8, body[after .. after + end_rel]);
}

test "integration: POST /v1/workspaces empty body assigns a Heroku-style name" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const r = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json("{}")).send();
    defer r.deinit();

    try r.expectStatus(.created);
    try std.testing.expect(r.bodyContains("\"workspace_id\""));
    try std.testing.expect(r.bodyContains("\"name\":\""));

    // The generator format is `<adjective>-<noun>-<3digit>`; any two-hyphen
    // string of reasonable length proves the body wired through.
    const name = (try extractName(alloc, r.body)) orelse return error.MissingNameField;
    defer alloc.free(name);
    try std.testing.expect(name.len >= 5);
    var hyphens: usize = 0;
    for (name) |c| if (c == '-') {
        hyphens += 1;
    };
    try std.testing.expect(hyphens >= 2);
}

test "integration: two consecutive empty-body POSTs return distinct names" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const r1 = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json("{}")).send();
    defer r1.deinit();
    try r1.expectStatus(.created);
    const name1 = (try extractName(alloc, r1.body)) orelse return error.MissingNameField;
    defer alloc.free(name1);

    const r2 = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json("{}")).send();
    defer r2.deinit();
    try r2.expectStatus(.created);
    const name2 = (try extractName(alloc, r2.body)) orelse return error.MissingNameField;
    defer alloc.free(name2);

    // The retry-on-collision path is silent on success; this assertion is
    // the only signal that the second POST didn't get the same name and
    // either skip the partial-unique-index path or 500 on it.
    try std.testing.expect(!std.mem.eql(u8, name1, name2));
}

test "integration: POST /v1/workspaces with explicit name stores it verbatim" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    // Use a unique-per-test-run name so re-runs against a persistent DB
    // don't fail on the partial unique index.
    const ts = clock.nowMillis();
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"explicit-{d}\"}}", .{ts});
    defer alloc.free(body);

    const r = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();

    try r.expectStatus(.created);
    const expected = try std.fmt.allocPrint(alloc, "\"name\":\"explicit-{d}\"", .{ts});
    defer alloc.free(expected);
    try std.testing.expect(r.bodyContains(expected));
}

test "integration: POST /v1/workspaces rejects duplicate name within tenant" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const ts = clock.nowMillis();
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"dup-{d}\"}}", .{ts});
    defer alloc.free(body);

    const r1 = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(body)).send();
    defer r1.deinit();
    try r1.expectStatus(.created);

    const r2 = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(body)).send();
    defer r2.deinit();
    // The handler does a single-attempt insert on caller-supplied names;
    // a unique-violation surfaces as the generic create_workspace failure
    // path (5xx). Pinning to >= 500 catches the contract today AND fails
    // loudly if a tenant-probe / auth path ever masks the real outcome
    // with a 401/422. Tightening to a 409 is a follow-up.
    try std.testing.expect(r2.status >= 500);
}

test "integration: POST /v1/workspaces without auth returns 401" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try (try h.post("/v1/workspaces").json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}
