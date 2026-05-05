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
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Reuse the tenant + signed token from tenant_workspaces_integration_test —
// same TEST_JWKS, so the rsa256 signature validates.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";

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
    try seedTenant(conn, std.time.milliTimestamp());

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
    try seedTenant(conn, std.time.milliTimestamp());

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
    try seedTenant(conn, std.time.milliTimestamp());

    // Use a unique-per-test-run name so re-runs against a persistent DB
    // don't fail on the partial unique index.
    const ts = std.time.milliTimestamp();
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
    try seedTenant(conn, std.time.milliTimestamp());

    const ts = std.time.milliTimestamp();
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
