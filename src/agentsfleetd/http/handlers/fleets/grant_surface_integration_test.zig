// The retired external-caller surface stays retired.
//
// Both dimensions are absence claims, and absence is exactly what a normal
// behavioural test cannot assert: there is no handler left to call. So these
// read the two places the surface could come back from — the live catalogue
// and the source tree itself — and fail if either grows it again.
//
// Requires TEST_DATABASE_URL for the catalogue half; skipped gracefully
// otherwise. The source half needs no database and always runs.

const std = @import("std");
const pg = @import("pg");
const base = @import("../../../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

/// Every table the fleet-key surface owned or keyed into. `core.fleet_keys` is
/// the table itself; the rest of the surface (routes, handler, CLI) is proven
/// by the source scan below, because a route leaves no catalogue trace.
const RETIRED_TABLE = "fleet_keys";

fn scalarI64(conn: *pg.Conn, sql: []const u8, arg: []const u8) !i64 {
    var q = @import("../../../db/pg_query.zig").PgQuery.from(try conn.query(sql, .{arg}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoRow;
    return row.get(i64, 0);
}

test "integration: test_fleet_keys_surface_fully_removed" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // The table is absent from the catalogue. Asserted against `pg_tables`
    // rather than by a failing SELECT: a SELECT that errors proves only that
    // THIS statement fails, while a catalogue count proves the slot never
    // installed it — which is what the teardown-rebuild posture (RULE SCH)
    // actually claims.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        db_ctx.conn,
        "SELECT count(*)::bigint FROM pg_tables WHERE schemaname = 'core' AND tablename = $1",
        RETIRED_TABLE,
    ));

    // And nothing references it — a foreign key pointing at a table that is
    // gone would have failed migration, but an index or constraint NAMED for
    // it would survive and mislead the next reader.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        db_ctx.conn,
        "SELECT count(*)::bigint FROM pg_indexes WHERE schemaname = 'core' AND indexname LIKE '%' || $1 || '%'",
        RETIRED_TABLE,
    ));
}

test "test_no_handler_local_authentication" {
    // The claim in one sentence: no route authenticates outside the
    // middleware chain. The handler-local lookup that did (`authenticateFleet`,
    // plus its `Session {uuid}` arm reading `core.fleet_sessions`) is gone, and
    // the only way to prove it stays gone is to read the sources that would
    // carry it back.
    //
    // Embedded rather than walked: `@embedFile` resolves at comptime against
    // paths that must exist, so a file RENAME breaks this test loudly instead
    // of silently scanning nothing — the failure mode a directory walk has.
    const route_table = @embedFile("../../route_table.zig");
    const routes = @embedFile("../../routes.zig");
    const router = @embedFile("../../router.zig");
    const invoke = @embedFile("../../route_table_invoke.zig");
    const webhooks = @embedFile("../../route_table_invoke_webhooks.zig");

    const retired = [_][]const u8{
        // The handler-local authenticator and its two credential arms.
        "authenticateFleet",
        "fleetFromSession",
        "fleetFromApiKey",
        // The route variants that reached them, and the duplicate approval path.
        "request_integration_grant",
        "grant_approval_webhook",
        // The fleet-key management surface that minted the credential.
        "delete_fleet_key",
        "invokeFleetKeys",
    };
    for ([_][]const u8{ route_table, routes, router, invoke, webhooks }) |source| {
        for (retired) |symbol| {
            if (std.mem.indexOf(u8, source, symbol) != null) {
                std.debug.print("retired auth symbol is back in the route plumbing: {s}\n", .{symbol});
                return error.RetiredAuthSurfaceReturned;
            }
        }
    }

    // The middleware chain is the ONLY authenticator left: every route spec
    // names a registry entry, so a route added with a bespoke lookup would have
    // to say `MiddlewareRegistry.none` and do the work itself. `none` is still
    // legitimate for genuinely unauthenticated surfaces (HMAC-signed webhook
    // ingress verifies its own signature), so this asserts the count has not
    // grown rather than that it is zero.
    var none_count: usize = 0;
    var it = std.mem.splitScalar(u8, route_table, '\n');
    while (it.next()) |line| {
        if (std.mem.indexOf(u8, line, "MiddlewareRegistry.none") != null) none_count += 1;
    }
    try std.testing.expectEqual(EXPECTED_UNAUTHENTICATED_ROUTES, none_count);
}

/// The unauthenticated routes that remain, each verifying its own signature:
/// the HMAC webhook receivers and the connector OAuth callbacks. Raising this
/// number means a new route opted out of the middleware chain — which is the
/// decision this test exists to make visible, so it must be a deliberate edit.
/// Dropped from 11 when the ops-class Prometheus pull route was retired with
/// its rendering layer (runtime metrics now ride the OTLP push exporter).
const EXPECTED_UNAUTHENTICATED_ROUTES: usize = 10;
