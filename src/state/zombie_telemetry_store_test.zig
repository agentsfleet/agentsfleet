// Integration tests for zombie_telemetry_store.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const store = @import("zombie_telemetry_store.zig");
const tenant_provider = @import("tenant_provider.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;
const WS_A = "0195b4ba-8d3a-7f13-8abc-aa1900000001";
const WS_B = "0195b4ba-8d3a-7f13-8abc-aa1900000002";
const ZOMBIE_A = "zombie-telem-a";
const ZOMBIE_B = "zombie-telem-b";
const PLATFORM_MODEL = tenant_provider.PLATFORM_DEFAULT_MODEL;

fn seedStageRow(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, event_id: []const u8, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .zombie_id = zombie_id,
        .event_id = event_id,
        .charge_type = .stage,
        .posture = .platform,
        .model = PLATFORM_MODEL,
        .credit_deducted_cents = 2,
        .recorded_at = recorded_at,
    });
}

fn seedReceiveRow(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, event_id: []const u8, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .zombie_id = zombie_id,
        .event_id = event_id,
        .charge_type = .receive,
        .posture = .platform,
        .model = PLATFORM_MODEL,
        .credit_deducted_cents = 1,
        .recorded_at = recorded_at,
    });
}

fn teardownTelemetry(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch {};
}

// ── Insert: idempotent on (event_id, charge_type) ───────────────────────────

test "insert_telemetry_idempotent_on_event_charge" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-idem-aa19-0001";
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000); // duplicate — no-op

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry WHERE workspace_id = $1 AND event_id = $2",
        .{ WS_A, evt },
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// ── Insert: receive + stage rows coexist for the same event_id ──────────────

test "insert_telemetry_two_rows_per_event" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-two-aa19-0001";
    try seedReceiveRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 2000);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry WHERE event_id = $1",
        .{evt},
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 2), try row.get(i64, 0));
}

// ── Insert: receive row has NULL token counts ───────────────────────────────

test "insert_receive_has_null_tokens_and_wall_ms" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-rcv-aa19-0001";
    try seedReceiveRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT token_count_input, token_count_output, wall_ms
        \\FROM zombie_execution_telemetry
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{evt}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 0));
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 1));
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 2));
}

// ── Update: stage tokens set post-execution ─────────────────────────────────

test "update_stage_tokens_writes_only_stage_row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-upd-aa19-0001";
    try seedReceiveRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 1000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, evt, 2000);

    try store.updateStageTokens(db_ctx.conn, evt, 800, 1040, 4200);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, token_count_input, token_count_output, wall_ms
        \\FROM zombie_execution_telemetry
        \\WHERE event_id = $1
        \\ORDER BY charge_type
    , .{evt}));
    defer q.deinit();

    const receive = (try q.next()).?;
    try std.testing.expectEqualStrings("receive", try receive.get([]const u8, 0));
    try std.testing.expectEqual(@as(?i64, null), try receive.get(?i64, 1));
    try std.testing.expectEqual(@as(?i64, null), try receive.get(?i64, 2));
    try std.testing.expectEqual(@as(?i64, null), try receive.get(?i64, 3));

    const stage = (try q.next()).?;
    try std.testing.expectEqualStrings("stage", try stage.get([]const u8, 0));
    try std.testing.expectEqual(@as(?i64, 800), try stage.get(?i64, 1));
    try std.testing.expectEqual(@as(?i64, 1040), try stage.get(?i64, 2));
    try std.testing.expectEqual(@as(?i64, 4200), try stage.get(?i64, 3));
}

// ── Customer query basic ────────────────────────────────────────────────────

test "list_for_zombie_returns_rows" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0001", 1000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0002", 2000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-list-aa19-0003", 3000);

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

    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-ws-aa19-0001", 1000);

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

    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, "zombie-nobody", 50, null);
    defer {
        for (rows) |*r| r.deinit(ALLOC);
        ALLOC.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 0), rows.len);
}

// ── Cursor pagination ───────────────────────────────────────────────────────

test "list_for_zombie_cursor_paginates" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    // Seed 5 rows with distinct recorded_at values (descending order: 5000..1000)
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0001", 1000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0002", 2000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0003", 3000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0004", 4000);
    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-page-aa19-0005", 5000);

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

// ── Leak detection ──────────────────────────────────────────────────────────

test "list_for_zombie_no_leak" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    try seedStageRow(db_ctx.conn, WS_A, ZOMBIE_A, "evt-lk1-aa19-0001", 1000);

    const rows = try store.listTelemetryForZombie(db_ctx.conn, ALLOC, WS_A, ZOMBIE_A, 50, null);
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
    // testing.allocator will detect any leak when the test completes
}
