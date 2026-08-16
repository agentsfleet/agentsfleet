//! Schema migration runner — applies versioned migrations under an advisory
//! lock and tracks per-version success/failure rows in `audit.schema_migrations`
//! and `audit.schema_migration_failures`. Split from `pool.zig` per RULE FLL.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const logging = @import("log");
const sql_splitter = @import("sql_splitter.zig");
const migration_versions = @import("migration_versions.zig");
const AppliedVersionSet = migration_versions.AppliedVersionSet;
const migration_lock = @import("pool_migration_lock.zig");
const migration_state = @import("pool_migration_state.zig");

const log = logging.scoped(.db_migrate);

const Conn = pg.Conn;
const Pool = pg.Pool;

/// `pool_types.zig` is the leaf that breaks the pool.zig ↔ pool_migrations.zig import cycle.
const types = @import("pool_types.zig");
const Migration = types.Migration;

const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";

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

fn refuseNoncanonicalSchemaVersion(conn: *Conn, migrations: []const Migration) !void {
    try AppliedVersionSet.ensureCanonical(conn, migrations);
}

fn applySqlStatements(conn: *Conn, version: i32, sql: []const u8) !u32 {
    // Loud reject: unterminated SQL fails as a named SplitError (never error.PG)
    // before any truncated statement could apply.
    sql_splitter.SqlStatementSplitter.validate(sql) catch |err| {
        log.err("migrate_sql_invalid", .{ .version = version, .err = @errorName(err) });
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

pub const inspectMigrationState = migration_state.inspectMigrationState;

/// Execute versioned schema migrations, once each, in order.
pub fn runMigrations(pool: *Pool, migrations: []const Migration) !void {
    return runMigrationsWithPolicy(
        pool,
        migrations,
        migration_lock.MAX_ATTEMPTS,
        migration_lock.RETRY_MS,
        .reap,
    );
}

/// Execute the canonical migration list while refusing a ledger from a newer
/// binary. The comparison and cleanup share the migration advisory lock.
pub fn runMigrationsRefusingNewer(pool: *Pool, migrations: []const Migration) !void {
    return runMigrationsWithPolicy(
        pool,
        migrations,
        migration_lock.MAX_ATTEMPTS,
        migration_lock.RETRY_MS,
        .refuse,
    );
}

/// Same run under an injected lock bound so tests fail fast — mirrors
/// `migration_lock.acquireBounded`. The advisory lock is taken BEFORE the
/// bookkeeping `ensure*` DDL: `CREATE ... IF NOT EXISTS` is not race-safe, so
/// fresh-database boots must serialize even the table creation.
pub fn runMigrationsBounded(pool: *Pool, migrations: []const Migration, lock_max_attempts: u32, lock_retry_ms: u64) !void {
    return runMigrationsWithPolicy(pool, migrations, lock_max_attempts, lock_retry_ms, .reap);
}

const AheadPolicy = enum {
    reap,
    refuse,
};

fn runMigrationsWithPolicy(
    pool: *Pool,
    migrations: []const Migration,
    lock_max_attempts: u32,
    lock_retry_ms: u64,
    ahead_policy: AheadPolicy,
) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    log.info("migrate_conn_acquired", .{ .expected_versions = migrations.len });

    migration_lock.acquireBounded(conn, lock_max_attempts, lock_retry_ms) catch |err| {
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.acquire_lock");
        return err;
    };
    defer migration_lock.release(conn);
    log.info("migrate_lock_acquired", .{});

    ensureSchemaMigrationsTable(conn) catch |err| {
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.ensure_schema_migrations_table");
        return err;
    };
    ensureSchemaMigrationFailuresTable(conn) catch |err| {
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.ensure_schema_migration_failures_table");
        return err;
    };

    if (ahead_policy == .refuse) {
        refuseNoncanonicalSchemaVersion(conn, migrations) catch |err| {
            if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.refuse_noncanonical_schema");
            return err;
        };
    }

    var reap_scratch: [REAP_SCRATCH_BYTES]u8 = undefined;
    var reap_fba = std.heap.FixedBufferAllocator.init(&reap_scratch);
    reapOrphanedMigrationRows(reap_fba.allocator(), conn, migrations) catch |err| {
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.reap_orphans");
        return err;
    };

    const applied = AppliedVersionSet.load(conn, migrations) catch |err| {
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.load_applied_versions");
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
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.begin_tx");
        return err;
    };
    const statements = applySqlStatements(conn, migration.version, migration.sql) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.apply_sql_statements");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };

    _ = conn.exec(
        "INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2)",
        .{ migration.version, clock.nowMillis() },
    ) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.insert_schema_migrations");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };

    _ = conn.exec(S_COMMIT, .{}) catch |err| {
        rollbackTx(conn);
        if (err == error.PG) migration_state.logPgErrorContext(conn, "migrate.commit_tx");
        markMigrationFailure(conn, migration.version, err);
        return err;
    };
    clearMigrationFailure(conn, migration.version);
    log.info("migration_applied", .{ .version = migration.version, .statements = statements });
}
