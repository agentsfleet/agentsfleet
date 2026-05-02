// Integration tests for recordZombieDelivery telemetry path.
//
// Split from metering_test.zig (RULE FLL: 350-line gate). Covers:
//   - negative epoch_wall_time_ms skips telemetry write
//   - zero epoch_wall_time_ms (gate-blocked) skips telemetry write
//   - positive epoch writes one stage row with correct fields
//   - same event_id called twice produces exactly one row (ON CONFLICT DO NOTHING)
//
// Requires real DB (TEST_DATABASE_URL or DATABASE_URL). Skips if not available.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const balance_policy = @import("../config/balance_policy.zig");
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

// negative epoch_wall_time_ms must not write a telemetry row.
// Spec: "Negative epoch is a system clock anomaly — log and skip rather than
// storing a corrupt row or passing a negative value to @intCast."
test "record_zombie_delivery_skips_telemetry_on_negative_epoch_wall_time_ms" {
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
        balance_policy.DEFAULT,
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_EPOCH_NEG);
    try std.testing.expectEqual(@as(i64, 0), count);
}

// zero epoch_wall_time_ms (gate-blocked event) must not write telemetry.
// Spec: "Skip gate-blocked events (epoch_wall_time_ms=0) — no meaningful wall time."
test "record_zombie_delivery_skips_telemetry_on_zero_epoch_wall_time_ms" {
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
        balance_policy.DEFAULT,
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_EPOCH_ZERO);
    try std.testing.expectEqual(@as(i64, 0), count);
}

// positive epoch writes exactly one telemetry row with correct field values.
test "record_zombie_delivery_writes_one_stage_row_on_successful_delivery" {
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
        30, // agent_seconds → wall_ms = 30000
        1420, // token_count
        870, // time_to_first_token_ms (no longer persisted)
        epoch, // positive epoch — telemetry must be written
        balance_policy.DEFAULT,
    );

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, model,
        \\       token_count_input, token_count_output, wall_ms,
        \\       event_id
        \\FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1
    , .{WS_TEL_EPOCH_POS}));
    defer q.deinit();
    const r = (try q.next()) orelse {
        try std.testing.expect(false); // no row written — test fails
        return;
    };
    try std.testing.expectEqualStrings("stage", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("platform", try r.get([]const u8, 1));
    try std.testing.expectEqualStrings(tenant_provider.PLATFORM_DEFAULT_MODEL, try r.get([]const u8, 2));
    try std.testing.expectEqual(@as(?i64, 0), try r.get(?i64, 3)); // token_count_input — placeholder until resolver lands
    try std.testing.expectEqual(@as(?i64, 1420), try r.get(?i64, 4)); // token_count_output ← token_count
    try std.testing.expectEqual(@as(?i64, 30_000), try r.get(?i64, 5)); // wall_ms ← agent_seconds * 1000
    try std.testing.expectEqualStrings(event_id, try r.get([]const u8, 6));
}

// telemetry insert is idempotent: same event_id replayed twice inserts 1 row.
// Mirrors the credit deduction idempotency contract (metering_test.zig)
// but for the telemetry table — UNIQUE (event_id, charge_type) + ON CONFLICT DO NOTHING.
test "record_zombie_delivery_telemetry_insert_is_idempotent_on_replay" {
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
        balance_policy.DEFAULT,
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
        balance_policy.DEFAULT,
    );

    const count = try countTelemetryRows(db_ctx.conn, WS_TEL_IDEMPOTENT);
    try std.testing.expectEqual(@as(i64, 1), count);
}
