// Integration tests for zombie_telemetry_store.zig — M18_001 (segment aa19*).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const store = @import("zombie_telemetry_store.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;
const WS_A = "0195b4ba-8d3a-7f13-8abc-aa1900000001";
const WS_B = "0195b4ba-8d3a-7f13-8abc-aa1900000002";
const ZOMBIE_A = "zombie-m18-telem-a";
const ZOMBIE_B = "zombie-m18-telem-b";

fn seedRow(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, event_id: []const u8, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .event_id = event_id,
        .token_count = 100,
        .time_to_first_token_ms = 500,
        .epoch_wall_time_ms = 1712924400000,
        .wall_seconds = 10,
        .plan_tier = "free",
        .credit_deducted_cents = 10,
        .recorded_at = recorded_at,
    });
}

fn teardownTelemetry(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch {};
}

// ── Dim 2.2: insert idempotent ───────────────────────────────────────────────

test "insert_telemetry_idempotent" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-idem-aa19-0001";
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000); // duplicate — should be no-op

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry WHERE workspace_id = $1 AND event_id = $2",
        .{ WS_A, evt },
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// ── Dim 2.3: zero TTFT allowed ──────────────────────────────────────────────

test "insert_zero_ttft_allowed" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-ttft0-aa19-0001";
    try store.insertTelemetry(db_ctx.conn, ALLOC, .{
        .zombie_id = ZOMBIE_A,
        .workspace_id = WS_A,
        .event_id = evt,
        .token_count = 50,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 1712924400000,
        .wall_seconds = 5,
        .plan_tier = "free",
        .credit_deducted_cents = 5,
        .recorded_at = 2000,
    });

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT time_to_first_token_ms FROM zombie_execution_telemetry WHERE event_id = $1",
        .{evt},
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
}

// ── Dim 3.1: customer query basic ───────────────────────────────────────────

test "list_for_zombie_returns_rows" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0002", 2000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0003", 3000);

    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 50, null);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }

    try std.testing.expectEqual(@as(usize, 3), rows.len);
    // newest first
    try std.testing.expect(rows[0].recorded_at >= rows[1].recorded_at);
    try std.testing.expect(rows[1].recorded_at >= rows[2].recorded_at);
}

test "list_for_zombie_wrong_workspace_returns_empty" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-ws-aa19-0001", 1000);

    // Query with WS_B (not seeded, not matching)
    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_B, ZOMBIE_A, 50, null);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 0), rows.len);
}

test "list_for_zombie_empty_returns_empty" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);

    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, "zombie-m18-nobody", 50, null);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 0), rows.len);
}

// ── Dim 3.2: cursor pagination ───────────────────────────────────────────────

test "list_for_zombie_cursor_paginates" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    // Seed 5 rows with distinct recorded_at values (descending order: 5000..1000)
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0002", 2000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0003", 3000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0004", 4000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0005", 5000);

    // Page 1: limit=2 → newest 2 rows
    const page1 = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 2, null);
    defer {
        for (page1) |*r| r.deinit(ALLOC);
        ALLOC.free(page1);
    }
    try std.testing.expectEqual(@as(usize, 2), page1.len);
    try std.testing.expectEqual(@as(i64, 5000), page1[0].recorded_at);
    try std.testing.expectEqual(@as(i64, 4000), page1[1].recorded_at);

    // Build cursor from last row of page 1
    const cursor = try store.makeCursor(ALLOC, page1[page1.len - 1]);
    defer ALLOC.free(cursor);

    // Page 2: limit=2 with cursor → next 2
    const page2 = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 2, cursor);
    defer {
        for (page2) |*r| r.deinit(ALLOC);
        ALLOC.free(page2);
    }
    try std.testing.expectEqual(@as(usize, 2), page2.len);
    try std.testing.expectEqual(@as(i64, 3000), page2[0].recorded_at);
    try std.testing.expectEqual(@as(i64, 2000), page2[1].recorded_at);

    // Page 3: cursor from last of page 2 → final 1
    const cursor2 = try store.makeCursor(ALLOC, page2[page2.len - 1]);
    defer ALLOC.free(cursor2);

    const page3 = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 2, cursor2);
    defer {
        for (page3) |*r| r.deinit(ALLOC);
        ALLOC.free(page3);
    }
    try std.testing.expectEqual(@as(usize, 1), page3.len);
    try std.testing.expectEqual(@as(i64, 1000), page3[0].recorded_at);
}

// ── Dim 4.1: operator cross-workspace ───────────────────────────────────────

test "list_all_no_filters_returns_all" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);
    try base.seedWorkspace(db_ctx.conn, WS_B);
    defer base.teardownWorkspace(db_ctx.conn, WS_B);
    defer teardownTelemetry(db_ctx.conn, WS_B);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-all-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_B, ZOMBIE_B, "evt-all-aa19-0002", 2000);

    const rows = try store.listTelemetryAll(db_ctx.conn, ALLOC, null, null, null, 50);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    // At minimum our 2 seeded rows must be present (other tests may add rows)
    try std.testing.expect(rows.len >= 2);
}

// ── Dim 4.2: workspace filter ────────────────────────────────────────────────

test "list_all_workspace_filter" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);
    try base.seedWorkspace(db_ctx.conn, WS_B);
    defer base.teardownWorkspace(db_ctx.conn, WS_B);
    defer teardownTelemetry(db_ctx.conn, WS_B);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-wsf-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-wsf-aa19-0002", 2000);
    try seedRow(db_ctx.conn, WS_B, ZOMBIE_B, "evt-wsf-aa19-0003", 3000);

    const rows = try store.listTelemetryAll(db_ctx.conn, ALLOC, WS_A, null, null, 50);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 2), rows.len);
    for (rows) |r| {
        try std.testing.expectEqualStrings(WS_A, r.workspace_id);
    }
}

// ── Dim 4.3: after filter ────────────────────────────────────────────────────

test "list_all_after_filter" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-aft-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-aft-aa19-0002", 5000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-aft-aa19-0003", 9000);

    // after=3000 → only rows with recorded_at > 3000
    const rows = try store.listTelemetryAll(db_ctx.conn, ALLOC, WS_A, null, 3000, 50);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 2), rows.len);
    for (rows) |r| {
        try std.testing.expect(r.recorded_at > 3000);
    }
}

// ── Operator zombie_id filter ────────────────────────────────────────────────

test "list_all_zombie_id_filter" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-zid-aa19-0001", 1000);
    try seedRow(db_ctx.conn, WS_A, ZOMBIE_B, "evt-zid-aa19-0002", 2000);

    const rows = try store.listTelemetryAll(db_ctx.conn, ALLOC, null, ZOMBIE_A, null, 50);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 1), rows.len);
    try std.testing.expectEqualStrings(ZOMBIE_A, rows[0].zombie_id);
}

// ── Leak detection ───────────────────────────────────────────────────────────

test "list_for_zombie_no_leak" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-lk1-aa19-0001", 1000);

    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 50, null);
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
    // testing.allocator will detect any leak when the test completes
}

test "list_all_no_leak" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-lk2-aa19-0001", 2000);

    const rows = try store.listTelemetryAll(db_ctx.conn, ALLOC, WS_A, null, null, 50);
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
}
