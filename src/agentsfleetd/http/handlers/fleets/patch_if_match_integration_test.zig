// HTTP integration tests for If-Match optimistic concurrency on the fleet
// PATCH (M131 §4). The GET's ETag header is not observable through this harness
// (fetch captures status + body only), but the PATCH echoes the current/fresh
// tag in its BODY on both the 412 and the 200 — so this suite round-trips the
// whole compute → attach → verdict path without needing header capture:
//
//   1. PATCH with no If-Match succeeds and returns an `etag` in the body.
//   2. PATCH with a STALE If-Match → 412 UZ-AGT-014 carrying the current `etag`.
//   3. PATCH with the tag from step 1 (matching) → 200.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const IFMATCH_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dddd1";
const TOKEN = scope_fixtures.OPERATOR; // fleet:write

const IF_MATCH = "if-match";
// A valid SKILL.md whose frontmatter name matches the seeded fleet; the PATCH
// reparses source_markdown and enforces the name invariant.
const SKILL_V2 =
    \\---
    \\name: ifmatch-fleet
    \\description: edited source
    \\version: 1.0.1
    \\---
    \\# Reviewer
    \\do the thing, edited
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seed(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'IfMatchTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
    // The seeded source's frontmatter name must equal the fleet name (the
    // reparse enforces SKILL.md name == fleet name).
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'ifmatch-fleet',
        \\  '---\nname: ifmatch-fleet\ndescription: seed\nversion: 1.0.0\n---\n# Reviewer\noriginal',
        \\  '{}'::jsonb, 'active', $3, $3)
        \\ON CONFLICT (id) DO UPDATE SET
        \\  source_markdown = EXCLUDED.source_markdown, name = EXCLUDED.name, status = 'active'
    , .{ IFMATCH_FLEET, TEST_WORKSPACE_ID, now_ms });
}

fn patchBody(alloc: std.mem.Allocator) ![]const u8 {
    return std.json.Stringify.valueAlloc(alloc, .{ .source_markdown = SKILL_V2 }, .{});
}

fn etagFromBody(alloc: std.mem.Allocator, body: []const u8) ![]const u8 {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const tag = parsed.value.object.get("etag") orelse return error.NoEtagInBody;
    return alloc.dupe(u8, tag.string);
}

test "integration: PATCH If-Match — success returns etag, stale is 412, match is 200" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seed(conn, now_ms);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, IFMATCH_FLEET });
    defer alloc.free(url);
    const body = try patchBody(alloc);
    defer alloc.free(body);

    // 1. No If-Match → succeeds (opt-in header), and the 200 carries a fresh etag.
    const r1 = try (try (try h.patch(url).bearer(TOKEN)).json(body)).send();
    defer r1.deinit();
    try r1.expectStatus(.ok);
    const fresh = try etagFromBody(alloc, r1.body);
    defer alloc.free(fresh);

    // 2. A stale If-Match → 412 UZ-AGT-014, and the body carries the CURRENT etag.
    const r2 = try (try (try (try h.patch(url).bearer(TOKEN)).json(body)).header(IF_MATCH, "\"deadbeef\"")).send();
    defer r2.deinit();
    try r2.expectStatus(.precondition_failed);
    try r2.expectErrorCode("UZ-AGT-014");
    const current = try etagFromBody(alloc, r2.body);
    defer alloc.free(current);
    // The 412's returned etag is the row's real tag — it equals the fresh tag
    // from step 1 (step 1 committed, and its returned etag is over that source).
    try std.testing.expectEqualStrings(fresh, current);

    // 3. Re-send with the matching tag → 200 (the editor's reloaded save).
    const r3 = try (try (try (try h.patch(url).bearer(TOKEN)).json(body)).header(IF_MATCH, current)).send();
    defer r3.deinit();
    try r3.expectStatus(.ok);
}
