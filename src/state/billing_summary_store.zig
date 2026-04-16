// M12_001: Billing summary aggregation over zombie_execution_telemetry.
//
// Shared by workspace summary (workspaces_billing_summary.zig) and per-zombie
// summary (zombie_billing_summary.zig). Same schema, two scopes.
//
// "completed" rows: credit_deducted_cents > 0 (billing-relevant deliveries).
// "non_billable" rows: credit_deducted_cents == 0 (score-gated or free-tier).
// agent_seconds comes from wall_seconds (best-effort; M15 may refine).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

pub const BillingSummary = struct {
    completed_count: i64,
    completed_agent_seconds: i64,
    non_billable_count: i64,
    total_runs: i64,
    total_cents: i64,
};

/// Aggregate telemetry across every zombie in a workspace for a time window.
pub fn aggregateForWorkspace(
    conn: *pg.Conn,
    workspace_id: []const u8,
    period_start_ms: i64,
) !BillingSummary {
    const sql =
        \\SELECT
        \\  COUNT(*) FILTER (WHERE credit_deducted_cents > 0)  AS completed_count,
        \\  COALESCE(SUM(wall_seconds) FILTER (WHERE credit_deducted_cents > 0), 0) AS completed_seconds,
        \\  COUNT(*) FILTER (WHERE credit_deducted_cents = 0)  AS non_billable_count,
        \\  COUNT(*)                                           AS total_runs,
        \\  COALESCE(SUM(credit_deducted_cents), 0)            AS total_cents
        \\FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1 AND recorded_at >= $2
    ;
    var q = PgQuery.from(try conn.query(sql, .{ workspace_id, period_start_ms }));
    defer q.deinit();
    return try readSummaryRow(&q);
}

/// Aggregate telemetry for a single zombie in a workspace for a time window.
pub fn aggregateForZombie(
    conn: *pg.Conn,
    workspace_id: []const u8,
    zombie_id: []const u8,
    period_start_ms: i64,
) !BillingSummary {
    const sql =
        \\SELECT
        \\  COUNT(*) FILTER (WHERE credit_deducted_cents > 0)  AS completed_count,
        \\  COALESCE(SUM(wall_seconds) FILTER (WHERE credit_deducted_cents > 0), 0) AS completed_seconds,
        \\  COUNT(*) FILTER (WHERE credit_deducted_cents = 0)  AS non_billable_count,
        \\  COUNT(*)                                           AS total_runs,
        \\  COALESCE(SUM(credit_deducted_cents), 0)            AS total_cents
        \\FROM zombie_execution_telemetry
        \\WHERE workspace_id = $1 AND zombie_id = $2 AND recorded_at >= $3
    ;
    var q = PgQuery.from(try conn.query(sql, .{ workspace_id, zombie_id, period_start_ms }));
    defer q.deinit();
    return try readSummaryRow(&q);
}

fn readSummaryRow(q: *PgQuery) !BillingSummary {
    // Aggregates always return exactly one row. If the row iterator yields
    // nothing, treat it as a database integrity failure — the caller maps to
    // an internal DB error code.
    const row = (try q.next()) orelse return error.NoAggregateRow;
    return .{
        .completed_count = try row.get(i64, 0),
        .completed_agent_seconds = try row.get(i64, 1),
        .non_billable_count = try row.get(i64, 2),
        .total_runs = try row.get(i64, 3),
        .total_cents = try row.get(i64, 4),
    };
}

// ── Unit tests ────────────────────────────────────────────────────────────

test "BillingSummary is POD with expected i64 fields" {
    const zero: BillingSummary = .{
        .completed_count = 0,
        .completed_agent_seconds = 0,
        .non_billable_count = 0,
        .total_runs = 0,
        .total_cents = 0,
    };
    try std.testing.expectEqual(@as(i64, 0), zero.completed_count);
    try std.testing.expectEqual(@as(i64, 0), zero.total_cents);
}

test "BillingSummary accepts non-zero values and preserves them" {
    const s: BillingSummary = .{
        .completed_count = 47,
        .completed_agent_seconds = 1350,
        .non_billable_count = 3,
        .total_runs = 50,
        .total_cents = 1240,
    };
    try std.testing.expectEqual(@as(i64, 50), s.total_runs);
    try std.testing.expectEqual(@as(i64, 1240), s.total_cents);
    try std.testing.expectEqual(@as(i64, 47), s.completed_count);
}
