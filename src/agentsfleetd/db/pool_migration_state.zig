//! Read-only migration ledger inspection for startup safety and diagnostics.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");
const migration_versions = @import("migration_versions.zig");
const AppliedVersionSet = migration_versions.AppliedVersionSet;
const migration_lock = @import("pool_migration_lock.zig");

const log = logging.scoped(.db_migrate);
const Conn = pg.Conn;
const Pool = pg.Pool;
const types = @import("pool_types.zig");
const Migration = types.Migration;
const MigrationState = types.MigrationState;

const S_PG_ERROR = "pg_error";
const S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMIT =
    "SELECT 1 FROM audit.schema_migration_failures LIMIT 1";
const S_SELECT_UNRESOLVED_MIGRATION_FAILURES =
    \\SELECT 1 FROM audit.schema_migration_failures f
    \\WHERE NOT EXISTS (
    \\    SELECT 1 FROM audit.schema_migrations m WHERE m.version = f.version
    \\)
    \\LIMIT 1
;

fn hasFailedMigrationRecords(conn: *Conn, correlate_applied: bool) !bool {
    const sql = if (correlate_applied)
        S_SELECT_UNRESOLVED_MIGRATION_FAILURES
    else
        S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMIT;
    var result = PgQuery.from(try conn.query(sql, .{}));
    defer result.deinit();
    return (try result.next()) != null;
}

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = PgQuery.from(try conn.query(
        "SELECT COALESCE(MAX(version), 0) FROM audit.schema_migrations",
        .{},
    ));
    defer result.deinit();
    const row = try result.next() orelse return 0;
    return row.get(i32, 0);
}

pub fn logPgErrorContext(conn: *Conn, op: []const u8) void {
    if (conn.err) |pg_err| {
        log.err(S_PG_ERROR, .{
            .op = op,
            .error_code = error_codes.ERR_INTERNAL_DB_QUERY,
            .pg_code = pg_err.code,
            .message = pg_err.message,
        });
        if (pg_err.detail) |detail| {
            log.err("pg_error_detail", .{ .op = op, .detail = detail });
        }
        if (pg_err.hint) |hint| {
            log.err("pg_error_hint", .{ .op = op, .hint = hint });
        }
        return;
    }
    log.err(S_PG_ERROR, .{
        .op = op,
        .error_code = error_codes.ERR_INTERNAL_DB_QUERY,
        .message = "unknown",
    });
}

fn isUndefinedTablePgError(conn: *Conn) bool {
    if (conn.err) |pg_err| {
        return std.mem.eql(u8, pg_err.code, "42P01");
    }
    return false;
}

fn tableExists(conn: *Conn, query_sql: []const u8) !bool {
    var result = PgQuery.from(conn.query(query_sql, .{}) catch |err| {
        if (err == error.PG and isUndefinedTablePgError(conn)) return false;
        return err;
    });
    defer result.deinit();

    _ = result.next() catch |err| {
        if (err == error.PG and isUndefinedTablePgError(conn)) return false;
        return err;
    };
    return true;
}

pub fn inspectMigrationState(pool: *Pool, migrations: []const Migration) !MigrationState {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const has_migrations = tableExists(
        conn,
        "SELECT 1 FROM audit.schema_migrations LIMIT 1",
    ) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migrations");
        return err;
    };
    const has_failures = tableExists(
        conn,
        S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMIT,
    ) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migration_failures");
        return err;
    };

    var applied_versions: u32 = 0;
    var latest_expected: i32 = 0;
    const applied = if (has_migrations)
        AppliedVersionSet.load(conn, migrations) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.load_applied_versions");
            return err;
        }
    else
        AppliedVersionSet{};
    for (migrations) |migration| {
        latest_expected = @max(latest_expected, migration.version);
        if (applied.contains(migration.version)) applied_versions += 1;
    }

    const latest_applied = if (has_migrations)
        maxAppliedMigrationVersion(conn) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.max_applied_version");
            return err;
        }
    else
        0;
    const failed = if (has_failures)
        hasFailedMigrationRecords(conn, has_migrations) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.has_failed_migrations");
            return err;
        }
    else
        false;

    var lock_available = true;
    if (applied_versions < migrations.len) {
        lock_available = migration_lock.probeAvailable(conn) catch false;
    }

    return .{
        .expected_versions = @intCast(migrations.len),
        .applied_versions = applied_versions,
        .latest_expected_version = latest_expected,
        .latest_applied_version = latest_applied,
        .has_failed_migrations = failed,
        .lock_available = lock_available,
        .has_newer_schema_version = applied.has_noncanonical or latest_applied > latest_expected,
    };
}
