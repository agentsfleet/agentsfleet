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
const detail_store = @import("../../../state/fleet_event_detail_store.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0afe01";
const FLEET_THREADED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aab01";
const FLEET_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aab02";
const FLEET_FAT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aab03";
// A workspace under a DIFFERENT tenant. `OTHER_WS_ID` shares this tenant, so
// it exercises the statement predicate; only a foreign tenant reaches the
// authorization deny.
const FOREIGN_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f02";
const FOREIGN_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0afe02";
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
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'ThreadForeignTenant', $2, $2)
        \\ON CONFLICT (id) DO NOTHING
    , .{ FOREIGN_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ FOREIGN_WS_ID, FOREIGN_TENANT_ID, now });
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
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id IN ($1, $2, $3)", .{ FLEET_THREADED, FLEET_FOREIGN, FLEET_FAT }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id IN ($1, $2, $3)", .{ FLEET_THREADED, FLEET_FOREIGN, FLEET_FAT }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE id IN ($1, $2)", .{ OTHER_WS_ID, FOREIGN_WS_ID }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
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
    { // an unsupported method on the shared route answers 405.
        // PUT carries a body by HTTP semantics and std asserts on a bodiless
        // send, so the body is required to reach the router at all.
        const r = try (try (try h.put(url).bearer(TOKEN_OPERATOR)).json("{}")).send();
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
    { // a workspace owned by another tenant is denied, not answered empty
        const url_denied = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages", .{ FOREIGN_WS_ID, FLEET_THREADED });
        defer ALLOC.free(url_denied);
        const r = try (try h.get(url_denied).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // a workspace id that is not a UUID is refused before any query runs
        const r = try (try h.get("/v1/workspaces/not-a-uuid/fleets/" ++ FLEET_THREADED ++ "/messages").bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // …and so is a fleet id that is not a UUID
        const r = try (try h.get("/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/fleets/not-a-uuid/messages").bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // a garbled starting_after is refused, never treated as "start from the
        // top" — a client that mangles a cursor must not silently get page one
        // back as though it were the page it asked for. `aGVsbG8` is valid
        // base64url that decodes to `hello`, so it clears the decode and dies
        // on the missing `created_at:event_id` separator: the parse arm, not
        // the alphabet arm.
        const url_bad = try std.fmt.allocPrint(ALLOC, "{s}?starting_after=aGVsbG8", .{url});
        defer ALLOC.free(url_bad);
        const r = try (try h.get(url_bad).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
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

/// Two of these bodies overflow the handler's page budget; one does not. That
/// gap is the whole point — page one must cut, and cutting must issue a cursor.
const FAT_BODY_BYTES: usize = 400 * 1024;
const FAT_TURNS: usize = 3;
/// Marker width is fixed so `bufPrint` fills the head exactly: `fat-N---`.
const FAT_MARKER_LEN: usize = 8;

fn seedFatThread(conn: *pg.Conn, alloc: std.mem.Allocator) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2), 'thread-fat', 'test', '{"name":"thread-fat"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_FAT, TEST_WORKSPACE_ID });

    const body = try alloc.alloc(u8, FAT_BODY_BYTES);
    defer alloc.free(body);
    @memset(body, 'x');

    var i: usize = 0;
    while (i < FAT_TURNS) : (i += 1) {
        _ = try std.fmt.bufPrint(body[0..FAT_MARKER_LEN], "fat-{d}---", .{i});
        var event_id_buf: [32]u8 = undefined;
        const event_id = try std.fmt.bufPrint(&event_id_buf, "180000000000{d}-0", .{i});
        _ = try conn.exec(
            \\INSERT INTO core.fleet_events
            \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
            \\   request_json, response_text, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'steer:user_test', 'chat', 'processed',
            \\        '{"message":"fat"}'::jsonb, $4, $5, $5)
            \\ON CONFLICT (fleet_id, event_id) DO NOTHING
        , .{ FLEET_FAT, TEST_WORKSPACE_ID, event_id, body, @as(i64, @intCast(1_800_000_000_000 + i)) });
    }
}

test "integration: a budget cut ships a short page WITH a cursor, never a silent truncation" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const seed_conn = try h.acquireConn();
        defer h.releaseConn(seed_conn);
        try seedFatThread(seed_conn, ALLOC);
    }

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages?limit=3", .{ TEST_WORKSPACE_ID, FLEET_FAT });
    defer ALLOC.free(url);

    var cursor_owned: ?[]u8 = null;
    defer if (cursor_owned) |c| ALLOC.free(c);
    { // asked for three, budget fits one: answered, not refused, and marked
        const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("fat-2---"));
        try std.testing.expect(!r.bodyContains("fat-1---"));
        // The guard this test exists for: `has_more` is computed against the
        // BUDGETED count, so the cut itself issues the cursor. Derive it from
        // the fetch count instead and the rest of the thread silently vanishes.
        try std.testing.expect(r.bodyContains("\"next_cursor\":\""));
        cursor_owned = try ALLOC.dupe(u8, try extractCursor(r.body));
    }
    { // following the cursor reaches the turn the budget cut — nothing is lost
        const url2 = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/messages?limit=3&starting_after={s}", .{ TEST_WORKSPACE_ID, FLEET_FAT, cursor_owned.? });
        defer ALLOC.free(url2);
        const r = try (try h.get(url2).bearer(TOKEN_VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("fat-1---"));
        try std.testing.expect(!r.bodyContains("fat-2---"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupTestData(conn);
}

/// Every allocation site inside the thread read, failed one at a time.
///
/// The read's cleanup is a fourteen-rung `errdefer` ladder in `readRow` plus
/// the ArrayList unwind in `listThreadForFleet` — cleanup that only runs when
/// an allocation fails, so an ordinary green test never touches a rung of it.
/// `checkAllAllocationFailures` fails each site in turn and asserts the read
/// leaked nothing on the way out. That is the only proof that every rung frees
/// what it claims to; reading the code and agreeing it "looks right" is not.
fn threadReadUnderAllocator(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    fleet_id: []const u8,
) !void {
    const rows = try detail_store.listThreadForFleet(
        conn,
        alloc,
        workspace_id,
        fleet_id,
        null,
        @intCast(SEEDED_EVENTS + 1),
    );
    detail_store.freeThreadRows(alloc, rows);
}

test "integration: every allocation site in the thread read unwinds without leaking" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    try std.testing.checkAllAllocationFailures(
        std.testing.allocator,
        threadReadUnderAllocator,
        .{ conn, TEST_WORKSPACE_ID, FLEET_THREADED },
    );

    cleanupTestData(conn);
}
