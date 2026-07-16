// HTTP integration tests for tenant memory forget (M131 §5):
//   DELETE /v1/workspaces/{ws}/fleets/{id}/memories/{key}   scope: fleet:write
//
// The operator's correction path — removes a memory the fleet learned wrong.
// Asserts: a present key is forgotten with 204 no-body; a missing key is a 404
// (never a silent success); a fleet cannot forget another fleet's key; and the
// forget is keyed on (fleet_id, key), not key alone; encoded separators
// round-trip, while malformed path encoding is rejected.
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

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f11";
// Two fleets in the same workspace, so the fleet-isolation assertion is a real
// cross-fleet forget attempt (not just cross-workspace).
const FLEET_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7181";
const FLEET_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7182";
const TOKEN = scope_fixtures.PATCH_CONCURRENT_ADMIN;
const TOKEN_READ_ONLY = scope_fixtures.VIEWER;
const CATEGORY_CORE = "core";
const SEED_TS_MS: i64 = 1_700_000_000_000;

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
        \\VALUES ($1, 'MemForgetTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
    const fleets = [_]struct { id: []const u8, name: []const u8 }{
        .{ .id = FLEET_A, .name = "mem-forget-a" },
        .{ .id = FLEET_B, .name = "mem-forget-b" },
    };
    for (fleets) |fleet| {
        _ = try conn.exec(
            \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', '{}'::jsonb, 'active', $4, $4)
            \\ON CONFLICT (id) DO NOTHING
        , .{ fleet.id, TEST_WORKSPACE_ID, fleet.name, now_ms });
    }
}

fn seedEntry(conn: *pg.Conn, fleet_id: []const u8, key: []const u8, content: []const u8) !void {
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer _ = conn.exec("RESET ROLE", .{}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    var id_buf: [128]u8 = undefined;
    const id = try std.fmt.bufPrint(&id_buf, "{s}:{s}", .{ fleet_id, key });
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, $7, $7)
        \\ON CONFLICT (key, fleet_id) DO UPDATE SET content = EXCLUDED.content
    , .{ uid, id, key, content, CATEGORY_CORE, fleet_id, SEED_TS_MS });
}

fn entryExists(conn: *pg.Conn, fleet_id: []const u8, key: []const u8) !bool {
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer _ = conn.exec("RESET ROLE", .{}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
    var q = PgQuery.from(try conn.query(
        "SELECT 1 FROM memory.memory_entries WHERE fleet_id = $1::uuid AND key = $2",
        .{ fleet_id, key },
    ));
    defer q.deinit();
    return (try q.next()) != null;
}

fn keyUrl(fleet_id: []const u8, key: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/memories/{s}", .{ TEST_WORKSPACE_ID, fleet_id, key });
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM memory.memory_entries WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ FLEET_A, FLEET_B }) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("RESET ROLE", .{}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id IN ($1::uuid, $2::uuid)", .{ FLEET_A, FLEET_B }) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
}

test "integration: forget removes the entry (204), 404s a missing key, isolates fleets" {
    const h = makeHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seed(conn, now_ms);
    cleanup(conn); // fresh slate
    try seed(conn, now_ms);

    // Same key on BOTH fleets — proves the DELETE keys on (fleet_id, key): fleet A's
    // forget must not touch fleet B's identically-keyed entry.
    try seedEntry(conn, FLEET_A, "wrong-lesson", "reviewers use tabs");
    try seedEntry(conn, FLEET_B, "wrong-lesson", "reviewers use tabs");
    try seedEntry(conn, FLEET_A, "path+plus", "literal plus");
    try seedEntry(conn, FLEET_A, "path space", "literal space");

    // Raw plus is literal in a path segment; it must not target the space key.
    const url_plus = try keyUrl(FLEET_A, "path+plus");
    defer ALLOC.free(url_plus);
    const plus = try (try h.delete(url_plus).bearer(TOKEN)).send();
    defer plus.deinit();
    try plus.expectStatus(.no_content);
    try std.testing.expect(!(try entryExists(conn, FLEET_A, "path+plus")));
    try std.testing.expect(try entryExists(conn, FLEET_A, "path space"));

    const url_space = try keyUrl(FLEET_A, "path%20space");
    defer ALLOC.free(url_space);
    const space = try (try h.delete(url_space).bearer(TOKEN)).send();
    defer space.deinit();
    try space.expectStatus(.no_content);
    try std.testing.expect(!(try entryExists(conn, FLEET_A, "path space")));

    // 1. A read-only token cannot mutate memory, and the entry survives.
    const url_a = try keyUrl(FLEET_A, "wrong-lesson");
    defer ALLOC.free(url_a);
    const denied = try (try h.delete(url_a).bearer(TOKEN_READ_ONLY)).send();
    defer denied.deinit();
    try denied.expectStatus(.forbidden);
    try std.testing.expect(try entryExists(conn, FLEET_A, "wrong-lesson"));

    // 2. Forget the present key → 204 no body, and the entry is gone.
    const r1 = try (try h.delete(url_a).bearer(TOKEN)).send();
    defer r1.deinit();
    try r1.expectStatus(.no_content);
    try std.testing.expectEqual(@as(usize, 0), r1.body.len); // 204 carries no body
    try std.testing.expect(!(try entryExists(conn, FLEET_A, "wrong-lesson")));

    // 3. Fleet isolation: fleet B's identically-keyed entry is untouched.
    try std.testing.expect(try entryExists(conn, FLEET_B, "wrong-lesson"));

    // 4. Encoded path separators round-trip to the stored memory key.
    try seedEntry(conn, FLEET_A, "style/key", "reviewers use spaces");
    const url_encoded = try keyUrl(FLEET_A, "style%2Fkey");
    defer ALLOC.free(url_encoded);
    const encoded = try (try h.delete(url_encoded).bearer(TOKEN)).send();
    defer encoded.deinit();
    try encoded.expectStatus(.no_content);
    try std.testing.expect(!(try entryExists(conn, FLEET_A, "style/key")));

    // 5. Forgetting an absent key → 404 (a mistype is surfaced, not swallowed).
    const url_missing = try keyUrl(FLEET_A, "never-existed");
    defer ALLOC.free(url_missing);
    const r2 = try (try h.delete(url_missing).bearer(TOKEN)).send();
    defer r2.deinit();
    try r2.expectStatus(.not_found);
    try r2.expectErrorCode("UZ-MEM-004");

    // 6. Malformed percent-encoding never reaches the database lookup.
    const url_invalid = try keyUrl(FLEET_A, "bad%2");
    defer ALLOC.free(url_invalid);
    const invalid = try (try h.delete(url_invalid).bearer(TOKEN)).send();
    defer invalid.deinit();
    try invalid.expectStatus(.bad_request);
    try invalid.expectErrorCode("UZ-REQ-001");

    cleanup(conn);
}
