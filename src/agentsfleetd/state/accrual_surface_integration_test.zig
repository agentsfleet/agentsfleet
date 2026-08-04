// The retired per-renewal accrual surface stays retired.
//
// This is an absence claim, and absence is what a behavioural test cannot
// assert: there is no handler left to call and no table left to read. So it
// reads the two places the surface could come back from — the live catalogue and
// the source tree — and fails if either grows it again. Same shape as the
// fleet-key retirement suite, for the same reason.
//
// What retired: `fleet.metering_periods` held one row per renewal slice, read by
// a store and a per-event operator endpoint. The credit-pool ledger answers the
// same questions from `billing.usage_ledger` without the per-slice row, so the
// table, its store and its endpoint went together. The ledger's OWN store
// (`fleet_telemetry_store.zig`) is a different module and legitimately survives —
// it backs the charges list, which is still served.
//
// Requires TEST_DATABASE_URL for the catalogue half; skipped gracefully
// otherwise. The source half needs no database and always runs.

const std = @import("std");
const pg = @import("pg");
const base = @import("../db/test_fixtures.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;

/// The accrual table itself. Its schema is `fleet` — it was a runtime detail
/// table, never a money table, which is part of why the ledger replaced it.
const RETIRED_TABLE = "metering_periods";

fn scalarI64(conn: *pg.Conn, sql: []const u8, arg: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{arg}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoRow;
    return row.get(i64, 0);
}

test "integration: test_accrual_surface_fully_removed" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Asserted against the catalogue rather than by a failing SELECT: a SELECT
    // that errors proves only that THIS statement fails, while a catalogue count
    // proves no slot ever installs it — which is what the teardown-rebuild
    // posture (RULE SCH) actually claims. Checked across every schema, because a
    // reintroduction would plausibly land in `billing` rather than `fleet`.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        db_ctx.conn,
        "SELECT count(*)::bigint FROM pg_tables WHERE tablename = $1",
        RETIRED_TABLE,
    ));

    // An index or constraint NAMED for the table would survive its drop and
    // mislead the next reader about what this database still holds.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        db_ctx.conn,
        "SELECT count(*)::bigint FROM pg_indexes WHERE indexname LIKE '%' || $1 || '%'",
        RETIRED_TABLE,
    ));
}

test "test_accrual_reader_and_route_are_gone_from_the_sources" {
    // Embedded rather than walked: `@embedFile` resolves at comptime against
    // paths that must exist, so a file RENAME breaks this test loudly instead of
    // silently scanning nothing — the failure mode a directory walk has.
    const route_table = @embedFile("../http/route_table.zig");
    const routes = @embedFile("../http/routes.zig");
    const billing_handler = @embedFile("../http/handlers/tenant_billing.zig");

    const retired = [_][]const u8{
        // The store the accrual endpoint read through.
        "fleet_metering_store",
        // The per-event accrual route and its handler entry point.
        "get_tenant_metering_periods",
        "innerGetTenantBillingTelemetry",
        // The table name itself, in any statement text.
        "metering_periods",
    };
    for ([_][]const u8{ route_table, routes, billing_handler }) |source| {
        for (retired) |symbol| {
            if (std.mem.indexOf(u8, source, symbol) != null) {
                std.debug.print("retired accrual symbol is back: {s}\n", .{symbol});
                return error.RetiredAccrualSurfaceReturned;
            }
        }
    }
}
