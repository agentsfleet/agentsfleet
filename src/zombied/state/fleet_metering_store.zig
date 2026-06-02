//! Read side of `fleet.metering_periods` — the per-renewal billing breakdown.
//!
//! The renewal CTE (`fleet/renewal.zig`) and the report settle INSERT one row
//! per `/renew` + a final settle row; this module reads an event's slices back,
//! ordered by `slice_seq`, for the billing drill-down behind the Usage tab's
//! one accumulated `stage` charge. The metering-periods table has no tenant
//! column, so the read is ownership-guarded by an EXISTS against the event's
//! `core.zombie_execution_telemetry` row (which the same CTE wrote with the
//! tenant id) — a tenant can only read the slices behind its own charge.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// One per-renewal (or settle) slice. All-numeric — `event_id` is the query
/// filter, not repeated per row; the response carries the slices in `slice_seq`
/// order so the caller renders the run's debit accrual renewal-by-renewal.
pub const MeteringPeriodRow = struct {
    slice_seq: i64,
    d_input_tokens: i64,
    d_cached_tokens: i64,
    d_output_tokens: i64,
    run_ms: i64,
    run_fee_nanos: i64,
    token_cost_nanos: i64,
    charged_nanos: i64,
    created_at: i64,
};

const LIST_FOR_EVENT_SQL =
    \\SELECT mp.slice_seq, mp.d_input_tokens, mp.d_cached_tokens, mp.d_output_tokens,
    \\       mp.run_ms, mp.run_fee_nanos, mp.token_cost_nanos, mp.charged_nanos, mp.created_at
    \\FROM fleet.metering_periods mp
    \\WHERE mp.event_id = $1
    \\  AND EXISTS (
    \\      SELECT 1 FROM core.zombie_execution_telemetry t
    \\      WHERE t.event_id = mp.event_id AND t.tenant_id = $2
    \\  )
    \\ORDER BY mp.slice_seq
;

/// Every metering slice for `event_id`, oldest-first, scoped to `tenant_id`:
/// the EXISTS guard means a foreign or unknown event returns an empty slice (no
/// cross-tenant leak, no 404 distinction). Rows are owned by `alloc` — all
/// scalar, so the slice is the only allocation (caller frees the slice; arena
/// callers free it with the request).
pub fn listForEvent(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    event_id: []const u8,
    tenant_id: []const u8,
) ![]MeteringPeriodRow {
    var q = PgQuery.from(try conn.query(LIST_FOR_EVENT_SQL, .{ event_id, tenant_id }));
    defer q.deinit();

    var rows: std.ArrayList(MeteringPeriodRow) = .{};
    errdefer rows.deinit(alloc);

    while (try q.next()) |row| {
        try rows.append(alloc, .{
            .slice_seq = try row.get(i64, 0),
            .d_input_tokens = try row.get(i64, 1),
            .d_cached_tokens = try row.get(i64, 2),
            .d_output_tokens = try row.get(i64, 3),
            .run_ms = try row.get(i64, 4),
            .run_fee_nanos = try row.get(i64, 5),
            .token_cost_nanos = try row.get(i64, 6),
            .charged_nanos = try row.get(i64, 7),
            .created_at = try row.get(i64, 8),
        });
    }
    return rows.toOwnedSlice(alloc);
}
