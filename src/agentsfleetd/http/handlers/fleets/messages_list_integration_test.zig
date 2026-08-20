// HTTP integration tests for the fleet chat-thread read
// (GET /v1/workspaces/{ws}/fleets/{id}/messages).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. No Redis needed:
// the thread read only touches Postgres.
//
// Uses the shared TestHarness (src/http/test_harness.zig).

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0afe01";
const FLEET_THREADED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aab01";
const FLEET_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aab02";
const SEEDED_EVENTS: usize = 5;

const TOKEN_VIEWER = scope_fixtures.VIEWER;
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'ThreadTest', $2, $2)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2), 'thread-fleet', 'test', '{"name":"thread-fleet"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_THREADED, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2), 'thread-foreign', 'test', '{"name":"thread-foreign"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_FOREIGN, OTHER_WS_ID });

    // Five chat turns, strictly ordered timestamps, bodies included.
    var i: usize = 0;
    while (i < SEEDED_EVENTS) : (i += 1) {
        var event_id_buf: [32]u8 = undefined;
        const event_id = try std.fmt.bufPrint(&event_id_buf, "170000000000{d}-0", .{i});
        var request_buf: [64]u8 = undefined;
        const request_json = try std.fmt.bufPrint(&request_buf, "{{\"message\":\"turn {d}\"}}", .{i});
        var response_buf: [32]u8 = undefined;
        const response_text = try std.fmt.bufPrint(&response_buf, "answer {d}", .{i});
        _ = try conn.exec(
            \\INSERT INTO core.fleet_events
            \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
            \\   request_json, response_text, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'steer:user_test', 'chat', 'processed',
            \\        $4::jsonb, $5, $6, $6)
            \\ON CONFLICT (fleet_id, event_id) DO NOTHING
        , .{ FLEET_THREADED, TEST_WORKSPACE_ID, event_id, request_json, response_text, @as(i64, @intCast(1_700_000_000_000 + i)) });
    }
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id IN ($1, $2)", .{ FLEET_THREADED, FLEET_FOREIGN }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id IN ($1, $2)", .{ FLEET_THREADED, FLEET_FOREIGN }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "integration: thread read returns newest-first bodies and pages without overlap" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages?limit=3", .{ TEST_WORKSPACE_ID, FLEET_THREADED });
    defer ALLOC.free(url);

    var cursor_owned: ?[]u8 = null;
    defer if (cursor_owned) |c| ALLOC.free(c);
    { // page one: newest three turns, bodies present, cursor issued
        const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"response_text\":\"answer 4\""));
        try std.testing.expect(r.bodyContains("\"answer 2\""));
        try std.testing.expect(!r.bodyContains("\"answer 1\""));
        try std.testing.expect(r.bodyContains("\"request_json\":"));
        try std.testing.expect(r.bodyContains("\"next_cursor\":\""));
        cursor_owned = try ALLOC.dupe(u8, try extractCursor(r.body));
    }
    { // page two continues from the cursor: the remaining two, no overlap, no cursor
        const url2 = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages?limit=3&starting_after={s}", .{ TEST_WORKSPACE_ID, FLEET_THREADED, cursor_owned.? });
        defer ALLOC.free(url2);
        const r = try (try h.get(url2).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"answer 1\""));
        try std.testing.expect(r.bodyContains("\"answer 0\""));
        try std.testing.expect(!r.bodyContains("\"answer 2\""));
        try std.testing.expect(r.bodyContains("\"next_cursor\":null"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

test "integration: thread read scoping, methods, and parameter refusals" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, FLEET_THREADED });
    defer ALLOC.free(url);

    { // a fleet:read-only credential reads the thread…
        const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }
    { // …but cannot steer: POST keeps the write scope
        const r = try (try (try h.post(url).bearer(TOKEN_VIEWER)).json("{\"message\":\"hi\"}")).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // an unsupported method on the shared route answers 405
        const r = try (try h.put(url).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.method_not_allowed);
    }
    { // a fleet id under another workspace yields an empty page — no existence leak
        const url_foreign = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ TEST_WORKSPACE_ID, FLEET_FOREIGN });
        defer ALLOC.free(url_foreign);
        const r = try (try h.get(url_foreign).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"items\":[]"));
    }
    { // limit out of range → 400
        const url_bad = try std.fmt.allocPrint(ALLOC, "{s}?limit=26", .{url});
        defer ALLOC.free(url_bad);
        const r = try (try h.get(url_bad).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // crafted cursor → 400
        const url_bad = try std.fmt.allocPrint(ALLOC, "{s}?starting_after=%%%not-base64", .{url});
        defer ALLOC.free(url_bad);
        const r = try (try h.get(url_bad).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

/// Pull the `next_cursor` value out of the response body. The cursor is
/// URL-safe base64 of `<ms>:<event_id>` — no escaping to undo.
fn extractCursor(body: []const u8) ![]const u8 {
    const key = "\"next_cursor\":\"";
    const start = (std.mem.indexOf(u8, body, key) orelse return error.TestUnexpectedResult) + key.len;
    const end = std.mem.indexOfScalarPos(u8, body, start, '"') orelse return error.TestUnexpectedResult;
    return body[start..end];
}
