//! Schema migration runner — applies versioned migrations under an advisory
//! lock and tracks per-version success/failure rows in `audit.schema_migrations`
//! and `audit.schema_migration_failures`. Split from `pool.zig` per RULE FLL.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");
const sql_splitter = @import("sql_splitter.zig");
const migration_versions = @import("migration_versions.zig");
const AppliedVersionSet = migration_versions.AppliedVersionSet;
const migration_lock = @import("pool_migration_lock.zig");

const log = logging.scoped(.db_migrate);

const Conn = pg.Conn;
const Pool = pg.Pool;

/// `pool_types.zig` is the leaf that breaks the pool.zig ↔ pool_migrations.zig import cycle.
const types = @import("pool_types.zig");
const Migration = types.Migration;
const MigrationState = types.MigrationState;

const S_PG_ERROR = "pg_error";
const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";
const S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMI = "SELECT 1 FROM audit.schema_migration_failures LIMIT 1";
// A failure row is fatal only while its version is unapplied; stale rows resolve.
const S_SELECT_UNRESOLVED_MIGRATION_FAILURES =
    \\SELECT 1 FROM audit.schema_migration_failures f
    \\WHERE NOT EXISTS (
    \\    SELECT 1 FROM audit.schema_migrations m WHERE m.version = f.version
    \\)
    \\LIMIT 1
;

// Stack scratch for reapOrphanedMigrationRows' IN-list (no heap): per-version
// decimal width × the version cap (migration_versions.zig) × live copies held
// at once (ArrayList growth ~2x + two rendered DELETEs) + template overhead.
const MAX_INLIST_DIGITS_PER_VERSION = 12;
const REAP_SQL_TEMPLATE_BYTES = 128;
const REAP_INLIST_COPIES = 4;
const REAP_SCRATCH_BYTES =
    migration_versions.MAX_TRACKED_MIGRATIONS * MAX_INLIST_DIGITS_PER_VERSION * REAP_INLIST_COPIES + REAP_SQL_TEMPLATE_BYTES * 2;

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

fn hasFailedMigrationRecords(conn: *Conn, correlate_applied: bool) !bool {
    // No schema_migrations table → nothing is applied → any failure row is
    // unresolved (and the correlated query would 42P01 on the missing table).
    const sql = if (correlate_applied) S_SELECT_UNRESOLVED_MIGRATION_FAILURES else S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMI;
    var result = PgQuery.from(try conn.query(sql, .{}));
    defer result.deinit();
    return (try result.next()) != null;
}

fn markMigrationFailure(conn: *Conn, version: i32, err: anyerror) void {
    const ts = clock.nowMillis();
    _ = conn.exec(
        \\INSERT INTO audit.schema_migration_failures (version, failed_at, error_text)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (version) DO UPDATE
        \\SET failed_at = EXCLUDED.failed_at,
        \\    error_text = EXCLUDED.error_text
    , .{ version, ts, @errorName(err) }) catch |xerr| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(xerr) });
}

fn clearMigrationFailure(conn: *Conn, version: i32) void {
    _ = conn.exec("DELETE FROM audit.schema_migration_failures WHERE version = $1", .{version}) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

/// Delete bookkeeping rows whose version left the canonical migration list
/// (pre-v2.0 teardown — RULE SCH). No-op on every other migrate run.
fn reapOrphanedMigrationRows(allocator: std.mem.Allocator, conn: *Conn, migrations: []const Migration) !void {
    // Empty list → `NOT IN ()` is a Postgres syntax error (42601); nothing to reap.
    if (migrations.len == 0) return;

    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(allocator);
    for (migrations, 0..) |m, i| {
        if (i > 0) try buf.append(allocator, ',');
        try buf.print(allocator, "{d}", .{m.version});
    }
    const canonical_list = buf.items;

    const reap_migrations_sql = try std.fmt.allocPrint(
        allocator,
        "DELETE FROM audit.schema_migrations WHERE version NOT IN ({s})",
        .{canonical_list},
    );
    defer allocator.free(reap_migrations_sql);
    const reaped = try conn.exec(reap_migrations_sql, .{});
    if (reaped != null and reaped.? > 0) {
        log.info("migration_reap", .{ .reaped = reaped.?, .scope = "orphan_rows" });
    }

    const reap_failures_sql = try std.fmt.allocPrint(
        allocator,
        "DELETE FROM audit.schema_migration_failures WHERE version NOT IN ({s})",
        .{canonical_list},
    );
    defer allocator.free(reap_failures_sql);
    _ = try conn.exec(reap_failures_sql, .{});
}

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = PgQuery.from(try conn.query("SELECT COALESCE(MAX(version), 0) FROM audit.schema_migrations", .{}));
    defer result.deinit();
    const row = try result.next() orelse return 0;
    return row.get(i32, 0);
}

fn logPgErrorContext(conn: *Conn, op: []const u8) void {
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

fn applySqlStatements(conn: *Conn, version: i32, sql: []const u8) !u32 {
    // Loud reject: unterminated SQL fails as a named SplitError (never error.PG)
    // before any truncated statement could apply.
    sql_splitter.SqlStatementSplitter.validate(sql) catch |err| {
        log.err("migrate.sql_invalid", .{ .version = version, .err = @errorName(err) });
        return err;
    };
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

fn rollbackTx(conn: *Conn) void {
    // conn.rollback() handles the FAIL state where exec("ROLLBACK") would silently no-op.
    conn.rollback() catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

pub fn inspectMigrationState(pool: *Pool, migrations: []const Migration) !MigrationState {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const has_schema_migrations = tableExists(conn, "SELECT 1 FROM audit.schema_migrations LIMIT 1") catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migrations");
        return err;
    };
    const has_schema_migration_failures = tableExists(conn, S_SELECT_1_FROM_AUDIT_SCHEMA_MIGRATION_FAILURES_LIMI) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "inspect.table_exists audit.schema_migration_failures");
        return err;
    };

    var applied_versions: u32 = 0;
    var latest_expected: i32 = 0;
    const applied = if (has_schema_migrations)
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

    const latest_applied = if (has_schema_migrations)
        maxAppliedMigrationVersion(conn) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.max_applied_version");
            return err;
        }
    else
        0;
    const failed = if (has_schema_migration_failures)
        hasFailedMigrationRecords(conn, has_schema_migrations) catch |err| {
            if (err == error.PG) logPgErrorContext(conn, "inspect.has_failed_migrations");
            return err;
        }
    else
        false;

    // Pooler-safe: probeAvailable's transaction-scoped advisory lock auto-releases
    // at statement end (a session-scoped probe leaks onto a pooled backend). Advisory
    // locks are cluster-wide, so a direct-connection migrator is still detected.
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
        .has_newer_schema_version = latest_applied > latest_expected,
    };
}

/// Execute versioned schema migrations, once each, in order.
pub fn runMigrations(pool: *Pool, migrations: []const Migration) !void {
    return runMigrationsBounded(pool, migrations, migration_lock.MAX_ATTEMPTS, migration_lock.RETRY_MS);
}

/// Same run under an injected lock bound so tests fail fast — mirrors
/// `migration_lock.acquireBounded`. The advisory lock is taken BEFORE the
/// bookkeeping `ensure*` DDL: `CREATE ... IF NOT EXISTS` is not race-safe, so
/// fresh-database boots must serialize even the table creation.
pub fn runMigrationsBounded(pool: *Pool, migrations: []const Migration, lock_max_attempts: u32, lock_retry_ms: u64) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    log.info("migrate.conn_acquired", .{ .expected_versions = migrations.len });

    migration_lock.acquireBounded(conn, lock_max_attempts, lock_retry_ms) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.acquire_lock");
        return err;
    };
    defer migration_lock.release(conn);
    log.info("migrate.lock_acquired", .{});

    ensureSchemaMigrationsTable(conn) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.ensure_schema_migrations_table");
        return err;
    };
    ensureSchemaMigrationFailuresTable(conn) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.ensure_schema_migration_failures_table");
        return err;
    };

    var reap_scratch: [REAP_SCRATCH_BYTES]u8 = undefined;
    var reap_fba = std.heap.FixedBufferAllocator.init(&reap_scratch);
    reapOrphanedMigrationRows(reap_fba.allocator(), conn, migrations) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.reap_orphans");
        return err;
    };

    const applied = AppliedVersionSet.load(conn, migrations) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.load_applied_versions");
        return err;
    };

    for (migrations) |migration| {
        if (applied.contains(migration.version)) {
            clearMigrationFailure(conn, migration.version);
            continue;
        }
        try applyOneMigration(conn, migration);
    }
}

/// Apply one migration in its own transaction, recording success/failure bookkeeping.
fn applyOneMigration(conn: *Conn, migration: Migration) !void {
    log.info("migration_start", .{ .version = migration.version });
    _ = conn.exec(S_BEGIN, .{}) catch |err| {
        if (err == error.PG) logPgErrorContext(conn, "migrate.begin_tx");
        return err;
    };
    const statements = applySqlStatements(conn, migration.version, migration.sql) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) logPgErrorContext(conn, "migrate.apply_sql_statements");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };

    _ = conn.exec(
        "INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2)",
        .{ migration.version, clock.nowMillis() },
    ) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) logPgErrorContext(conn, "migrate.insert_schema_migrations");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };

    _ = conn.exec(S_COMMIT, .{}) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) logPgErrorContext(conn, "migrate.commit_tx");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };
    clearMigrationFailure(conn, migration.version);
    log.info("migration_applied", .{ .version = migration.version, .statements = statements });
}
