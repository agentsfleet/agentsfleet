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

