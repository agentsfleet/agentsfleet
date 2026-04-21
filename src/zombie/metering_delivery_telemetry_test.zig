// M18_001: Integration tests for recordZombieDelivery telemetry path.
//
// Split from metering_test.zig (RULE FLL: 350-line gate). Covers:
//   T1  — negative epoch_wall_time_ms skips telemetry write
//   T2  — zero epoch_wall_time_ms (gate-blocked) skips telemetry write
//   T3  — positive epoch writes one row with correct fields
//   T4  — same event_id called twice produces exactly one row (ON CONFLICT DO NOTHING)
//
// Requires real DB (TEST_DATABASE_URL or DATABASE_URL). Skips if not available.
// Workspace IDs use segment aa18* to isolate M18 test rows.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_TEL_EPOCH_NEG = "0195b4ba-8d3a-7f13-8abc-aa1800000001";
const WS_TEL_EPOCH_ZERO = "0195b4ba-8d3a-7f13-8abc-aa1800000002";
const WS_TEL_EPOCH_POS = "0195b4ba-8d3a-7f13-8abc-aa1800000003";
const WS_TEL_IDEMPOTENT = "0195b4ba-8d3a-7f13-8abc-aa1800000004";

/// Seed workspace + tenant-billing row so the metering path can resolve
/// tenant_id and plan_tier. Without the tenant row, `recordZombieDelivery`
/// returns early on tenant lookup and no telemetry would be written, masking
/// the epoch-guard behavior under test.
fn seedTelemetryWorkspace(conn: *pg.Conn, ws_id: []const u8) !void {
    try uc1.seed(conn, ws_id);
    try tenant_billing.provisionFreeDefault(conn, uc1.TENANT_ID);
}

fn teardownTelemetryWorkspace(conn: *pg.Conn, ws_id: []const u8) void {
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{ws_id}) catch {};
    uc1.teardown(conn, ws_id);
}

fn countTelemetryRows(conn: *pg.Conn, ws_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT FROM zombie_execution_telemetry WHERE workspace_id = $1
    , .{ws_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    return r.get(i64, 0);
}

// T1 — negative epoch_wall_time_ms must not write a telemetry row.
// Spec: "Negative epoch is a system clock anomaly — log and skip rather than
// storing a corrupt row or passing a negative value to @intCast."
test "M18_001: recordZombieDelivery skips telemetry on negative epoch_wall_time_ms" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seedTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_NEG);
    defer teardownTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_NEG);

    metering.recordZombieDelivery(
        db_ctx.pool,
        ALLOC,
        WS_TEL_EPOCH_NEG,
        "zombie-tel-neg",
        "0195b4ba-8d3a-7f13-8abc-aa1800000e01",
        30,
        100,
        500,
        -1, // negative epoch — must skip telemetry write
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_EPOCH_NEG);
    try std.testing.expectEqual(@as(i64, 0), count);
}

// T2 — zero epoch_wall_time_ms (gate-blocked event) must not write telemetry.
// Spec: "Skip gate-blocked events (epoch_wall_time_ms=0) — no meaningful wall time."
test "M18_001: recordZombieDelivery skips telemetry on zero epoch_wall_time_ms" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seedTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_ZERO);
    defer teardownTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_ZERO);

    metering.recordZombieDelivery(
        db_ctx.pool,
        ALLOC,
        WS_TEL_EPOCH_ZERO,
        "zombie-tel-zero",
        "0195b4ba-8d3a-7f13-8abc-aa1800000e02",
        30,
        100,
        500,
        0, // zero epoch — gate-blocked, must skip telemetry write
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_EPOCH_ZERO);
    try std.testing.expectEqual(@as(i64, 0), count);
}

// T3 — positive epoch writes exactly one telemetry row with correct field values.
// Spec dim 2.4: "zombie_execution_telemetry has one row with matching fields after call."
test "M18_001: recordZombieDelivery writes one telemetry row on successful delivery" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seedTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_POS);
    defer teardownTelemetryWorkspace(db_ctx.conn, WS_TEL_EPOCH_POS);

    const epoch: i64 = 1712924400000;
    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1800000e03";

    metering.recordZombieDelivery(
        db_ctx.pool,
        ALLOC,
        WS_TEL_EPOCH_POS,
        "zombie-tel-pos",
        event_id,
        30, // agent_seconds → wall_seconds
        1420, // token_count
        870, // time_to_first_token_ms
        epoch, // positive epoch — telemetry must be written
    );

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT token_count, time_to_first_token_ms, epoch_wall_time_ms,
        \\       wall_seconds, plan_tier, event_id
        \\FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1
    , .{WS_TEL_EPOCH_POS}));
    defer q.deinit();
    const r = (try q.next()) orelse {
        try std.testing.expect(false); // no row written — test fails
        return;
    };
    try std.testing.expectEqual(@as(i64, 1420), try r.get(i64, 0)); // token_count
    try std.testing.expectEqual(@as(i64, 870), try r.get(i64, 1)); // time_to_first_token_ms
    try std.testing.expectEqual(epoch, try r.get(i64, 2)); // epoch_wall_time_ms
    try std.testing.expectEqual(@as(i64, 30), try r.get(i64, 3)); // wall_seconds
    try std.testing.expectEqualStrings("free", try r.get([]const u8, 4)); // plan_tier
    try std.testing.expectEqualStrings(event_id, try r.get([]const u8, 5)); // event_id
}

// T4 — telemetry insert is idempotent: same event_id replayed twice inserts 1 row.
// Spec dim 2.2: "second call with same event_id is a no-op (ON CONFLICT DO NOTHING)."
// Mirrors the credit deduction idempotency contract (metering_test.zig T6) but for
// the telemetry table.
test "M18_001: recordZombieDelivery telemetry insert is idempotent on replay" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seedTelemetryWorkspace(db_ctx.conn, WS_TEL_IDEMPOTENT);
    defer teardownTelemetryWorkspace(db_ctx.conn, WS_TEL_IDEMPOTENT);

    const epoch: i64 = 1712924400000;
    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1800000e04";

    // First delivery — should write one row.
    metering.recordZombieDelivery(
        db_ctx.pool,
        ALLOC,
        WS_TEL_IDEMPOTENT,
        "zombie-tel-idem",
        event_id,
        30,
        100,
        500,
        epoch,
    );

    // Replay: same event_id, different agent_seconds. ON CONFLICT DO NOTHING must
    // suppress the second insert — the original row must remain unchanged.
    metering.recordZombieDelivery(
        db_ctx.pool,
        ALLOC,
        WS_TEL_IDEMPOTENT,
        "zombie-tel-idem",
        event_id,
        45, // different agent_seconds from replay — must NOT produce a second row
        200,
        600,
        epoch + 1000,
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_IDEMPOTENT);
    try std.testing.expectEqual(@as(i64, 1), count);
}
