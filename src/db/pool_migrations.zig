//! Schema migration runner — applies versioned migrations under an advisory
//! lock and tracks per-version success/failure rows in `audit.schema_migrations`
//! and `audit.schema_migration_failures`. Split from `pool.zig` per RULE FLL.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");
const sql_splitter = @import("sql_splitter.zig");

const log = logging.scoped(.db_migrate);

const Conn = pg.Conn;
const Pool = pg.Pool;

/// Authoritative declarations live in `pool_types.zig` (the leaf module
/// that breaks the pool.zig ↔ pool_migrations.zig import cycle).
const types = @import("pool_types.zig");
const Migration = types.Migration;
const MigrationState = types.MigrationState;

const MigrationAdvisoryLockKey: i64 = 0x7A6F6D6269650001;

fn ensureAuditSchema(conn: *Conn) !void {
    _ = try conn.exec("CREATE SCHEMA IF NOT EXISTS audit", .{});
}

fn ensureSchemaMigrationsTable(conn: *Conn) !void {
    try ensureAuditSchema(conn);
    _ = try conn.exec(
        \\CREATE TABLE IF NOT EXISTS audit.schema_migrations (
        \\    version     INTEGER PRIMARY KEY,
        \\    applied_at  BIGINT NOT NULL
        \\)
    , .{});
}

fn ensureSchemaMigrationFailuresTable(conn: *Conn) !void {
    _ = try conn.exec(
        \\CREATE TABLE IF NOT EXISTS audit.schema_migration_failures (
        \\    version     INTEGER PRIMARY KEY,
        \\    failed_at   BIGINT NOT NULL,
        \\    error_text  TEXT NOT NULL
        \\)
    , .{});
}

fn isMigrationApplied(conn: *Conn, version: i32) !bool {
    var result = PgQuery.from(try conn.query(
        "SELECT 1 FROM audit.schema_migrations WHERE version = $1",
        .{version},
    ));
    defer result.deinit();
    return (try result.next()) != null;
}

fn hasFailedMigrationRecords(conn: *Conn) !bool {
    var result = PgQuery.from(try conn.query(
        "SELECT 1 FROM audit.schema_migration_failures LIMIT 1",
        .{},
    ));
    defer result.deinit();
    return (try result.next()) != null;
}

fn tryAcquireMigrationLock(conn: *Conn) !bool {
    var result = PgQuery.from(try conn.query("SELECT pg_try_advisory_lock($1)", .{MigrationAdvisoryLockKey}));
    defer result.deinit();
    const row = try result.next() orelse return false;
    return try row.get(bool, 0);
}

fn acquireMigrationLock(conn: *Conn) !void {
    var result = PgQuery.from(try conn.query("SELECT pg_advisory_lock($1)", .{MigrationAdvisoryLockKey}));
    defer result.deinit();
    _ = try result.next();
}

fn releaseMigrationLock(conn: *Conn) void {
    var result = PgQuery.from(conn.query("SELECT pg_advisory_unlock($1)", .{MigrationAdvisoryLockKey}) catch return);
    result.deinit();
}

fn markMigrationFailure(conn: *Conn, version: i32, err: anyerror) void {
    const ts = std.time.milliTimestamp();
    _ = conn.exec(
        \\INSERT INTO audit.schema_migration_failures (version, failed_at, error_text)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (version) DO UPDATE
        \\SET failed_at = EXCLUDED.failed_at,
        \\    error_text = EXCLUDED.error_text
    , .{ version, ts, @errorName(err) }) catch {};
}

fn clearMigrationFailure(conn: *Conn, version: i32) void {
    _ = conn.exec("DELETE FROM audit.schema_migration_failures WHERE version = $1", .{version}) catch {};
}

/// Delete rows in audit.schema_migrations + schema_migration_failures whose
/// version is no longer in the canonical migration list. Keeps the bookkeeping
/// table in sync when migrations are removed (pre-v2.0 teardown — RULE SCH).
/// Safe to run on every migrate: a fresh DB with no orphan rows is a no-op.
fn reapOrphanedMigrationRows(conn: *Conn, migrations: []const Migration) !void {
    // Empty list → `DELETE … WHERE version NOT IN ()` is a Postgres syntax
    // error (sqlstate 42601). A no-migrations boot has nothing to reap.
    if (migrations.len == 0) return;

    const allocator = std.heap.page_allocator;
    var buf = std.ArrayList(u8){};
    defer buf.deinit(allocator);
    var writer = buf.writer(allocator);
    for (migrations, 0..) |m, i| {
        if (i > 0) try writer.writeByte(',');
        try writer.print("{d}", .{m.version});
    }
    const canonical_list = buf.items;

    const reap_migrations_sql = try std.fmt.allocPrint(
        allocator,
        "DELETE FROM audit.schema_migrations WHERE version NOT IN ({s})",
        .{canonical_list},
    );
    defer allocator.free(reap_migrations_sql);
    const reaped = conn.exec(reap_migrations_sql, .{}) catch |err| return err;
    if (reaped != null and reaped.? > 0) {
        log.info("migration_reap", .{ .reaped = reaped.?, .scope = "orphan_rows" });
    }

    const reap_failures_sql = try std.fmt.allocPrint(
        allocator,
        "DELETE FROM audit.schema_migration_failures WHERE version NOT IN ({s})",
        .{canonical_list},
    );
    defer allocator.free(reap_failures_sql);
    _ = conn.exec(reap_failures_sql, .{}) catch |err| return err;
}

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = PgQuery.from(try conn.query("SELECT COALESCE(MAX(version), 0) FROM audit.schema_migrations", .{}));
    defer result.deinit();
    const row = try result.next() orelse return 0;
    return try row.get(i32, 0);
}

fn logPgErrorContext(conn: *Conn, op: []const u8) void {
    if (conn.err) |pg_err| {
        log.err("pg_error", .{
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
    log.err("pg_error", .{
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

fn applySqlStatements(conn: *Conn, sql: []const u8) !u32 {
    var splitter = sql_splitter.SqlStatementSplitter.init(sql);
    var count: u32 = 0;

    while (splitter.next()) |stmt| {
        const preview_len = @min(stmt.len, 120);
        log.debug("migrate_stmt", .{ .index = count + 1, .preview = stmt[0..preview_len] });
        _ = try conn.exec(stmt, .{});
        count += 1;
    }

    return count;
}

fn beginTx(conn: *Conn) !void {
    _ = try conn.exec("BEGIN", .{});
}

fn commitTx(conn: *Conn) !void {
    _ = try conn.exec("COMMIT", .{});
}

fn rollbackTx(conn: *Conn) void {
    // conn.rollback() handles the FAIL-state case where exec("ROLLBACK")
    // would silently no-op and leave the session in an aborted tx.
    conn.rollback() catch {};
}

pub fn inspectMigrationState(pool: *Pool, migrations: []const Migration) !MigrationState {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const has_schema_migrations = tableExists(conn, "SELECT 1 FROM audit.schema_migrations LIMIT 1") catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migrations");
        return err;
    };
    const has_schema_migration_failures = tableExists(conn, "SELECT 1 FROM audit.schema_migration_failures LIMIT 1") catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migration_failures");
        return err;
    };

    var applied_versions: u32 = 0;
    var latest_expected: i32 = 0;
    if (has_schema_migrations) {
        for (migrations) |migration| {
            latest_expected = @max(latest_expected, migration.version);
            if (isMigrationApplied(conn, migration.version) catch |err| {
                if (err == error.PG) logPgErrorContext(conn, "inspect.is_migration_applied");
                return err;
            }) {
                applied_versions += 1;
            }
        }
    } else {
        for (migrations) |migration| {
            latest_expected = @max(latest_expected, migration.version);
        }
    }

    const latest_applied = if (has_schema_migrations)
        maxAppliedMigrationVersion(conn) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.max_applied_version");
            return err;
        }
    else
        0;
    const failed = if (has_schema_migration_failures)
        hasFailedMigrationRecords(conn) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.has_failed_migrations");
            return err;
        }
    else
        false;

    var lock_available = true;
    if (applied_versions < migrations.len) {
        lock_available = tryAcquireMigrationLock(conn) catch false;
        if (lock_available) releaseMigrationLock(conn);
    }

    return .{
        .expected_versions = @intCast(migrations.len),
        .applied_versions = applied_versions,
        .latest_expected_version = latest_expected,
        .latest_applied_version = latest_applied,
        .has_failed_migrations = failed,
        .lock_available = lock_available,
        .has_newer_schema_version = latest_applied > latest_expected,
    };
}

/// Execute versioned schema migrations, once each, in order.
pub fn runMigrations(pool: *Pool, migrations: []const Migration) !void {
    var conn = try pool.acquire();
    defer pool.release(conn);

    ensureSchemaMigrationsTable(conn) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.ensure_schema_migrations_table");
        return err;
    };
    ensureSchemaMigrationFailuresTable(conn) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.ensure_schema_migration_failures_table");
        return err;
    };

    acquireMigrationLock(conn) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.acquire_lock");
        return err;
    };
    defer releaseMigrationLock(conn);

    reapOrphanedMigrationRows(conn, migrations) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.reap_orphans");
        return err;
    };

    for (migrations) |migration| {
        if (isMigrationApplied(conn, migration.version) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "migrate.is_migration_applied");
            return err;
        }) {
            clearMigrationFailure(conn, migration.version);
            continue;
        }

        beginTx(conn) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "migrate.begin_tx");
            return err;
        };
        const statements = applySqlStatements(conn, migration.sql) catch |err| {
            rollbackTx(conn);
            if (err == error.PG) logPgErrorContext(conn, "migrate.apply_sql_statements");
            markMigrationFailure(conn, migration.version, err);
            return err;
        };

        _ = conn.exec(
            "INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2)",
            .{ migration.version, std.time.milliTimestamp() },
        ) catch |err| {
            rollbackTx(conn);
            if (err == error.PG) logPgErrorContext(conn, "migrate.insert_schema_migrations");
            markMigrationFailure(conn, migration.version, err);
            return err;
        };

        commitTx(conn) catch |err| {
            rollbackTx(conn);
            if (err == error.PG) logPgErrorContext(conn, "migrate.commit_tx");
            markMigrationFailure(conn, migration.version, err);
            return err;
        };
        clearMigrationFailure(conn, migration.version);
        log.info("migration_applied", .{ .version = migration.version, .statements = statements });
    }
}
