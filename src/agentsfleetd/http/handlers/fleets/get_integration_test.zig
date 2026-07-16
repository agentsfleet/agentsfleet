// HTTP integration tests for GET /v1/workspaces/{ws}/fleets/{id} — the
// single-fleet detail read (M131 §1). Exercises full-detail serialization
// (including a NULL bundle_content_hash), the ETag response header, the
// 404-not-403 cross-workspace boundary, and scope closure.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const etag_mod = @import("../../etag.zig");

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// A second workspace under the SAME tenant, so a cross-workspace read is
// authorized-at-the-tenant yet must still 404 (the fleet is not in the path ws).
const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22";

const TOKEN_VIEWER = scope_fixtures.VIEWER; // fleet:read
const TOKEN_ADMIN = scope_fixtures.TENANT_ADMIN; // fleet:admin (closes over read)
const TOKEN_WITHOUT_FLEET_SCOPE = scope_fixtures.PLATFORM_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedWorkspaces(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'GetDetailTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    for ([_][]const u8{ TEST_WORKSPACE_ID, OTHER_WORKSPACE_ID }) |ws| {
        _ = try conn.exec(
            \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
            \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
        , .{ ws, TEST_TENANT_ID, now_ms });
    }
}

/// Insert one fleet with explicit source/trigger/bundle values, returning the
/// caller-owned id. `trigger_md`/`bundle_hash` may be null (both are nullable
/// columns the read must serialize as JSON null).
fn seedFleet(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    source_md: []const u8,
    trigger_md: ?[]const u8,
    bundle_hash: ?[]const u8,
    now_ms: i64,
) ![]const u8 {
    const id = try id_format.generateFleetId(alloc);
    errdefer alloc.free(id);
    const name = try std.fmt.allocPrint(alloc, "get-detail-{d}", .{now_ms});
    defer alloc.free(name);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, bundle_content_hash, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, '{}'::jsonb, 'active', $6, $7, $7)
    , .{ id, workspace_id, name, source_md, trigger_md, bundle_hash, now_ms });
    return id;
}

test "integration: get fleet serializes full detail incl. null bundle hash + ETag" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspaces(conn, now_ms);

    const source_md = "# SKILL\nreview pull requests\n";
    const id = try seedFleet(alloc, conn, TEST_WORKSPACE_ID, source_md, null, null, now_ms);
    defer alloc.free(id);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, id });
    defer alloc.free(url);
    const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const expected_etag = try etag_mod.compute(alloc, &.{ source_md, null });
    defer alloc.free(expected_etag);
    try std.testing.expectEqualStrings(expected_etag, r.header(etag_mod.HEADER_ETAG).?);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, r.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;

    try std.testing.expectEqualStrings(id, obj.get("id").?.string);
    try std.testing.expectEqualStrings("active", obj.get("status").?.string);
    try std.testing.expectEqualStrings(source_md, obj.get("source_markdown").?.string);
    // Both nullable columns serialize as JSON null, not an error, not omitted.
    try std.testing.expect(obj.get("trigger_markdown").? == .null);
    try std.testing.expect(obj.get("bundle_content_hash").? == .null);
    // Server-truth aggregates ride the read (this fleet has neither → 0, not null).
    try std.testing.expectEqual(@as(i64, 0), obj.get("events_processed").?.integer);
    try std.testing.expectEqual(@as(i64, 0), obj.get("budget_used_nanos").?.integer);
}

test "integration: get fleet — missing id and cross-workspace both 404 (never 403)" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspaces(conn, now_ms);

    // A well-formed id that names no fleet → 404.
    const ghost = try id_format.generateFleetId(alloc);
    defer alloc.free(ghost);
    const url_ghost = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, ghost });
    defer alloc.free(url_ghost);
    const r_ghost = try (try h.get(url_ghost).bearer(TOKEN_VIEWER)).send();
    defer r_ghost.deinit();
    try r_ghost.expectStatus(.not_found);

    // A real fleet that lives in ANOTHER workspace (same tenant, so the caller is
    // authorized at the tenant) → still 404, never 403. Existence is not leaked.
    const other_id = try seedFleet(alloc, conn, OTHER_WORKSPACE_ID, "# other", null, null, now_ms);
    defer alloc.free(other_id);
    const url_cross = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, other_id });
    defer alloc.free(url_cross);
    const r_cross = try (try h.get(url_cross).bearer(TOKEN_VIEWER)).send();
    defer r_cross.deinit();
    try r_cross.expectStatus(.not_found);
}

test "integration: get fleet — fleet:admin satisfies the fleet:read route (scope closure)" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspaces(conn, now_ms);
    const id = try seedFleet(alloc, conn, TEST_WORKSPACE_ID, "# skill", null, null, now_ms);
    defer alloc.free(id);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, id });
    defer alloc.free(url);
    const denied = try (try h.get(url).bearer(TOKEN_WITHOUT_FLEET_SCOPE)).send();
    defer denied.deinit();
    try denied.expectStatus(.forbidden);

    // fleet:admin closes over fleet:read — the read gate is fleet:read, and a
    // stronger scope satisfies it.
    const r = try (try h.get(url).bearer(TOKEN_ADMIN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
}
