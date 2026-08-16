// Every filter combination the event-history store branches on, and the
// allocation unwind underneath them.
//
// `listForFleet` and `listForWorkspace` each fan out into a distinct SQL
// statement per (cursor?, since?, actor?) combination — six branches across the
// two, of which only the unfiltered ones were ever driven. A filter that
// silently stopped applying would have returned a plausible page and passed
// every test, because nothing asserted which rows a filtered page holds.
//
// The second half drives `readRow`'s errdefer ladder with a failing allocator.
// It duplicates ten strings per row, each with its own errdefer, and a gap
// anywhere in that chain leaks on the OOM path — which `std.testing.allocator`
// reports here rather than in production under memory pressure.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");

const store = @import("fleet_events_store.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TENANT_ID = "0196a100-0000-7000-8000-00000000f001";
const WS_ID = "0196a100-0000-7000-8000-00000000f002";
const FLEET_A = "0196a100-0000-7000-8000-00000000f003";
const FLEET_B = "0196a100-0000-7000-8000-00000000f004";

const ACTOR_STEER = "steer:alice";
const ACTOR_WEBHOOK = "webhook:github";
/// What `globToLike` produces for the client's `steer:*`.
const ACTOR_LIKE_STEER = "steer:%";

// Fixed rather than clock-derived: the cursor and the since window are both
// assertions about ordering, and a per-run clock makes the expected page a
// moving target.
const BASE_MS: i64 = 1_760_000_000_000;
const FLEET_A_FIRST_MS: i64 = BASE_MS + 1_000;
const FLEET_A_SECOND_MS: i64 = BASE_MS + 2_000;
const FLEET_A_THIRD_MS: i64 = BASE_MS + 3_000;
const FLEET_B_FIRST_MS: i64 = BASE_MS + 4_000;
const FLEET_B_SECOND_MS: i64 = BASE_MS + 5_000;

const EV_A1 = "evt-fef-a1";
const EV_A2 = "evt-fef-a2";
const EV_A3 = "evt-fef-a3";
const EV_B1 = "evt-fef-b1";
const EV_B2 = "evt-fef-b2";

const PAGE = 50;

/// `request_json` is NOT NULL — the body is the event's payload, and the read
/// side never projects it, so any well-formed object serves.
const REQUEST_JSON = "{\"message\":\"filter fixture\"}";

fn seedEvent(
    conn: *pg.Conn,
    fleet_id: []const u8,
    event_id: []const u8,
    actor: []const u8,
    created_at: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, tokens, wall_ms, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, 'chat', 'processed',
        \\        $5::jsonb, 128, 4200, $6, $6)
    , .{ fleet_id, event_id, WS_ID, actor, REQUEST_JSON, created_at });
}

fn seed(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "fleet-events-filters");
    try base.seedWorkspaceWithTenant(conn, WS_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_A, WS_ID, "events-filter-a", "{}", "# a");
    try base.seedFleet(conn, FLEET_B, WS_ID, "events-filter-b", "{}", "# b");

    try seedEvent(conn, FLEET_A, EV_A1, ACTOR_STEER, FLEET_A_FIRST_MS);
    try seedEvent(conn, FLEET_A, EV_A2, ACTOR_WEBHOOK, FLEET_A_SECOND_MS);
    try seedEvent(conn, FLEET_A, EV_A3, ACTOR_STEER, FLEET_A_THIRD_MS);
    try seedEvent(conn, FLEET_B, EV_B1, ACTOR_STEER, FLEET_B_FIRST_MS);
    try seedEvent(conn, FLEET_B, EV_B2, ACTOR_WEBHOOK, FLEET_B_SECOND_MS);
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{WS_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownFleets(conn, WS_ID);
    base.teardownWorkspace(conn, WS_ID);
    base.teardownTenantById(conn, TENANT_ID);
}

fn freeRows(rows: []store.EventRow) void {
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
}

/// Assert a page holds exactly these event ids, newest-first.
fn expectPage(rows: []store.EventRow, expected: []const []const u8) !void {
    try std.testing.expectEqual(expected.len, rows.len);
    for (expected, rows) |want, got| try std.testing.expectEqualStrings(want, got.event_id);
}

test "integration: per-fleet listing applies the actor filter, the since window, and both behind a cursor" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);
    const conn = db_ctx.conn;

    // Unfiltered: the fleet's own three, newest-first, and never fleet B's.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{ .limit = PAGE });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A2, EV_A1 });
    }
    // Actor filter alone drops the webhook event from the middle of the page —
    // a filter that stopped applying would still return a plausible-looking two
    // rows if it merely truncated, so the ids are asserted, not the count.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{
            .limit = PAGE,
            .actor_like = ACTOR_LIKE_STEER,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A1 });
    }
    // Since window alone: strictly `>=`, so the boundary event is included.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{
            .limit = PAGE,
            .since_ms = FLEET_A_SECOND_MS,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A2 });
    }
    // Since window AND actor filter — the branch that composes them.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{
            .limit = PAGE,
            .since_ms = FLEET_A_FIRST_MS,
            .actor_like = ACTOR_LIKE_STEER,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A1 });
    }

    const cursor = try store.makeCursor(ALLOC, FLEET_A_THIRD_MS, EV_A3);
    defer ALLOC.free(cursor);

    // Cursor alone: strictly older than (created_at, event_id), so the anchor
    // itself never repeats on the next page.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{
            .limit = PAGE,
            .cursor = cursor,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A2, EV_A1 });
    }
    // Cursor AND actor filter — paging while filtered, the branch a client hits
    // on the second page of a filtered view.
    {
        const rows = try store.listForFleet(conn, ALLOC, WS_ID, FLEET_A, .{
            .limit = PAGE,
            .cursor = cursor,
            .actor_like = ACTOR_LIKE_STEER,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{EV_A1});
    }
}

test "integration: workspace aggregate spans both fleets and keeps its filters behind a cursor" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);
    const conn = db_ctx.conn;

    // The aggregate: every fleet in the workspace, one ordering.
    {
        const rows = try store.listForWorkspace(conn, ALLOC, WS_ID, null, .{ .limit = PAGE });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_B2, EV_B1, EV_A3, EV_A2, EV_A1 });
    }
    // A fleet_id filter delegates to the per-fleet path rather than growing a
    // seventh statement.
    {
        const rows = try store.listForWorkspace(conn, ALLOC, WS_ID, FLEET_B, .{ .limit = PAGE });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_B2, EV_B1 });
    }
    // Since window and actor filter, composed across fleets.
    {
        const rows = try store.listForWorkspace(conn, ALLOC, WS_ID, null, .{
            .limit = PAGE,
            .since_ms = FLEET_A_THIRD_MS,
            .actor_like = ACTOR_LIKE_STEER,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_B1, EV_A3 });
    }

    const cursor = try store.makeCursor(ALLOC, FLEET_B_FIRST_MS, EV_B1);
    defer ALLOC.free(cursor);

    // Cursor alone across the aggregate.
    {
        const rows = try store.listForWorkspace(conn, ALLOC, WS_ID, null, .{
            .limit = PAGE,
            .cursor = cursor,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A2, EV_A1 });
    }
    // Cursor AND actor filter across the aggregate.
    {
        const rows = try store.listForWorkspace(conn, ALLOC, WS_ID, null, .{
            .limit = PAGE,
            .cursor = cursor,
            .actor_like = ACTOR_LIKE_STEER,
        });
        defer freeRows(rows);
        try expectPage(rows, &.{ EV_A3, EV_A1 });
    }
}

test "integration: a page that runs out of memory mid-row frees every string it took" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);

    // Each row duplicates ten strings, every one behind its own errdefer, and
    // the row list has an errdefer of its own over the rows already appended.
    // Failing at each allocation index in turn walks the whole ladder: a gap
    // anywhere leaks, and `std.testing.allocator` fails this test rather than
    // letting it reach production as a slow drip under memory pressure.
    var fail_index: usize = 0;
    var saw_oom = false;
    while (fail_index < 40) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = fail_index });
        const result = store.listForFleet(
            db_ctx.conn,
            failing.allocator(),
            WS_ID,
            FLEET_A,
            .{ .limit = PAGE },
        );
        if (result) |rows| {
            // Past the last allocation the call succeeds; free through the same
            // allocator that served it and stop.
            for (rows) |*r| r.deinit(failing.allocator());
            failing.allocator().free(rows);
            break;
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            saw_oom = true;
        }
    }
    try std.testing.expect(saw_oom);
}
