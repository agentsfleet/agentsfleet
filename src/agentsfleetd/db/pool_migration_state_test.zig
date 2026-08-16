//! What an operator reads when PostgreSQL refuses the migration inspector.
//!
//! Every assertion drives a REAL server error rather than a fabricated one. The
//! fields being logged — `code`, `message`, `detail`, `hint` — are populated by
//! the server and by nothing else, so a hand-built error struct would prove the
//! logger compiles and nothing about what an operator reads at three in the
//! morning when a migration will not apply.
//!
//! Each test reads the emitted record back through a buffered sink and asserts
//! the diagnosis is IN it. Calling the logger and checking nothing would run the
//! lines without proving the operator learns anything — a migration that fails
//! with the code swallowed leaves "migration failed" and no next step.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const db = @import("pool.zig");
const pool_types = @import("pool_types.zig");
const logging = @import("log");
const base = @import("test_fixtures.zig");
const migration_state = @import("pool_migration_state.zig");
const error_codes = @import("../errors/error_registry.zig");

const ALLOC = std.testing.allocator;

// 42P01 undefined_table — the one code the inspector special-cases, because a
// fresh database legitimately has no `audit.schema_migrations` yet.
const MISSING_TABLE_SQL = "SELECT 1 FROM audit.a_table_that_does_not_exist";
// 42703 undefined_column against a real catalogue table. PostgreSQL attaches a
// HINT here that the undefined-table error does not carry.
const MISSING_COLUMN_SQL = "SELECT no_such_column FROM pg_catalog.pg_class LIMIT 1";
// 22012 division_by_zero — a runtime fault rather than a parse-time one, so the
// error arrives from execution instead of planning.
const DIVISION_SQL = "SELECT 1/0";

const OP = "test.migration_state";

/// Run `body` with a buffered sink installed and return everything emitted.
/// Caller frees. Mirrors `src/lib/logging/mod_test.zig`'s helper rather than
/// inventing a second capture shape.
fn capture(body: anytype) ![]u8 {
    var bs = logging.sinks.BufferedSink.init(ALLOC);
    defer bs.deinit();

    logging.sinks.clearSinksForTest();
    defer logging.sinks.clearSinksForTest();
    logging.sinks.registerSink(bs.sink());

    body();

    return bs.snapshot();
}

fn expectContains(haystack: []const u8, needle: []const u8) !void {
    if (std.mem.indexOf(u8, haystack, needle) == null) {
        std.debug.print("\nexpected: {s}\nin: {s}\n", .{ needle, haystack });
        return error.DiagnosisMissingFromLog;
    }
}

test "integration: a refused query puts its PostgreSQL code in front of the operator" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expectError(error.PG, db_ctx.conn.query(MISSING_TABLE_SQL, .{})); // check-pg-drain: ok — the query RETURNS an error, so no Result exists to drain
    const pg_err = db_ctx.conn.err orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("42P01", pg_err.code);

    const Ctx = struct {
        var conn: *pg.Conn = undefined;
        fn call() void {
            migration_state.logPgErrorContext(conn, OP);
        }
    };
    Ctx.conn = db_ctx.conn;
    const out = try capture(Ctx.call);
    defer ALLOC.free(out);

    // The operator must get the op, the registry code and the server's own code
    // and message — that quartet is the whole diagnosis.
    try expectContains(out, "op=" ++ OP);
    try expectContains(out, error_codes.ERR_INTERNAL_DB_QUERY);
    try expectContains(out, "pg_code=42P01");
    try expectContains(out, "message=");
    // The relation the server named must survive into the record, or the
    // operator knows a table was missing but not which one.
    try expectContains(out, "a_table_that_does_not_exist");
}

test "integration: a server hint is logged as its own record, not dropped" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expectError(error.PG, db_ctx.conn.query(MISSING_COLUMN_SQL, .{})); // check-pg-drain: ok — the query RETURNS an error, so no Result exists to drain
    const pg_err = db_ctx.conn.err orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("42703", pg_err.code);
    // Guard the premise: if this server build stops attaching a hint, the test
    // must say so rather than silently asserting nothing about the hint arm.
    if (pg_err.hint == null) return error.SkipZigTest;

    const Ctx = struct {
        var conn: *pg.Conn = undefined;
        fn call() void {
            migration_state.logPgErrorContext(conn, OP);
        }
    };
    Ctx.conn = db_ctx.conn;
    const out = try capture(Ctx.call);
    defer ALLOC.free(out);

    try expectContains(out, "pg_code=42703");
    try expectContains(out, "event=pg_error_hint");
    try expectContains(out, "hint=");
}

test "integration: a runtime fault reaches the log with its own code" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expectError(error.PG, db_ctx.conn.query(DIVISION_SQL, .{})); // check-pg-drain: ok — the query RETURNS an error, so no Result exists to drain
    const pg_err = db_ctx.conn.err orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("22012", pg_err.code);

    const Ctx = struct {
        var conn: *pg.Conn = undefined;
        fn call() void {
            migration_state.logPgErrorContext(conn, OP);
        }
    };
    Ctx.conn = db_ctx.conn;
    const out = try capture(Ctx.call);
    defer ALLOC.free(out);

    // A parse-time and a runtime failure must be distinguishable in the log —
    // they need different operator responses.
    try expectContains(out, "pg_code=22012");
    try expectContains(out, "division by zero");
}

// A database that has never been migrated. `audit.schema_migrations` does not
// exist there, which is the ONE PostgreSQL error the inspector is required to
// swallow — every other failure must propagate.
const SCRATCH_DB = "agentsfleetdb_migration_state_probe";

/// Swap the database name in a libpq URL, preserving credentials and query
/// string. Returned slice is owned by the caller.
fn urlForDatabase(alloc: std.mem.Allocator, url: []const u8, database: []const u8) ![]u8 {
    const query_start = std.mem.indexOfScalar(u8, url, '?') orelse url.len;
    const authority = url[0..query_start];
    const slash = std.mem.lastIndexOfScalar(u8, authority, '/') orelse return error.InvalidDatabaseUrl;
    return std.fmt.allocPrint(alloc, "{s}/{s}{s}", .{
        authority[0..slash],
        database,
        url[query_start..],
    });
}

test "integration: a database that was never migrated reports a clean slate, not an error" {
    const url = common.env.testLiveValue("TEST_DATABASE_URL") orelse return error.SkipZigTest;

    const admin = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer admin.pool.deinit();
    defer admin.pool.release(admin.conn);

    // Explicit cleanup in the body, not via defer: a deferred drop would run
    // while the scratch pool still holds a connection and PostgreSQL would
    // refuse it.
    _ = admin.conn.exec("DROP DATABASE IF EXISTS " ++ SCRATCH_DB, .{}) catch
        return error.SkipZigTest;
    _ = admin.conn.exec("CREATE DATABASE " ++ SCRATCH_DB, .{}) catch
        return error.SkipZigTest;

    const scratch_url = try urlForDatabase(ALLOC, url, SCRATCH_DB);
    defer ALLOC.free(scratch_url);

    const state = probeFreshDatabase(scratch_url);
    _ = admin.conn.exec("DROP DATABASE IF EXISTS " ++ SCRATCH_DB, .{}) catch {};

    const inspected = try state;
    // The bootstrap contract: an unmigrated database is inspectable and reads
    // as "nothing applied", so the migrator proceeds instead of refusing to
    // start. Before this test, the arm that swallows 42P01 had no coverage at
    // all — a change making the inspector propagate it would have broken every
    // first boot against a fresh database and passed the suite.
    try std.testing.expectEqual(@as(u32, 0), inspected.applied_versions);
    try std.testing.expectEqual(@as(i32, 0), inspected.latest_applied_version);
    try std.testing.expect(!inspected.has_failed_migrations);
    try std.testing.expect(!inspected.has_newer_schema_version);
}

/// Open a pool against `url` and inspect it with no expected migrations.
/// Split out so the scratch database is dropped even when this fails.
fn probeFreshDatabase(url: []const u8) !pool_types.MigrationState {
    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(common.globalIo(), ALLOC, opts);
    defer pool.deinit();
    return migration_state.inspectMigrationState(pool, &.{});
}

test "integration: a connection with no server error still names the operation" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // The fallback arm. The caller reaches the logger from an `if (err ==
    // error.PG)` branch, and a transport-level failure can set that error with
    // no server payload behind it. Silence there leaves the operator with a
    // failed migration and an empty log.
    try std.testing.expect(db_ctx.conn.err == null);

    const Ctx = struct {
        var conn: *pg.Conn = undefined;
        fn call() void {
            migration_state.logPgErrorContext(conn, OP);
        }
    };
    Ctx.conn = db_ctx.conn;
    const out = try capture(Ctx.call);
    defer ALLOC.free(out);

    try expectContains(out, "op=" ++ OP);
    try expectContains(out, error_codes.ERR_INTERNAL_DB_QUERY);
    // Explicitly "unknown" rather than an absent field: a missing key reads as
    // a logging bug, a present "unknown" reads as "the server said nothing".
    try expectContains(out, "message=unknown");
    // The no-error arm must NOT invent a server code it never received.
    try std.testing.expect(std.mem.indexOf(u8, out, "pg_code") == null);
}
