//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.

const std = @import("std");
const pg = @import("pg");
const id_format = @import("../types/id_format.zig");
const error_codes = @import("../errors/codes.zig");
const log = std.log.scoped(.db);

pub const Pool = pg.Pool;
pub const Conn = pg.Conn;
pub const Row = pg.Row;

pub const DbRole = enum {
    default,
    api,
    worker,
    callback,
    migrator,
};

pub fn roleEnvVarName(role: DbRole) []const u8 {
    return switch (role) {
        .api => "DATABASE_URL_API",
        .worker => "DATABASE_URL_WORKER",
        .callback => "DATABASE_URL_CALLBACK",
        .migrator => "DATABASE_URL_MIGRATOR",
        .default => "DATABASE_URL",
    };
}

pub const Config = struct {
    url: []const u8,
    pool_size: u32 = 4,
    timeout_ms: u32 = 10_000,
};

pub const Migration = struct {
    version: i32,
    sql: []const u8,
};

pub const MigrationState = struct {
    expected_versions: u32,
    applied_versions: u32,
    latest_expected_version: i32,
    latest_applied_version: i32,
    has_failed_migrations: bool,
    lock_available: bool,
    has_newer_schema_version: bool,
};

const MigrationAdvisoryLockKey: i64 = 0x7A6F6D6269650001;

/// Parse a Postgres connection URL into pg.Pool.Opts.
/// URL format: postgres://user:pass@host:port/dbname[?query]
/// TLS is always required — all role-separated connections go to hosted Postgres
/// providers (PlanetScale, Neon, Supabase) that mandate TLS.
pub fn parseUrl(alloc: std.mem.Allocator, url: []const u8) !pg.Pool.Opts {
    // strip scheme
    const rest = if (std.mem.startsWith(u8, url, "postgres://"))
        url["postgres://".len..]
    else if (std.mem.startsWith(u8, url, "postgresql://"))
        url["postgresql://".len..]
    else
        return error.InvalidDatabaseUrl;

    // user:pass@host:port/dbname[?query]
    const at_pos = std.mem.lastIndexOfScalar(u8, rest, '@') orelse return error.InvalidDatabaseUrl;
    const userpass = rest[0..at_pos];
    const hostpath = rest[at_pos + 1 ..];

    // user and password
    var username: []const u8 = "";
    var password: []const u8 = "";
    if (std.mem.indexOfScalar(u8, userpass, ':')) |colon| {
        username = userpass[0..colon];
        password = userpass[colon + 1 ..];
    } else {
        username = userpass;
    }

    // host:port/dbname[?query]
    const slash_pos = std.mem.indexOfScalar(u8, hostpath, '/') orelse return error.InvalidDatabaseUrl;
    const hostport = hostpath[0..slash_pos];
    const dbpath = hostpath[slash_pos + 1 ..];

    // Split dbname from query string (e.g. "mydb?sslmode=require" → "mydb", "sslmode=require")
    const query_start = std.mem.indexOfScalar(u8, dbpath, '?');
    const dbname = if (query_start) |q| dbpath[0..q] else dbpath;
    const query_string = if (query_start) |q| dbpath[q + 1 ..] else "";

    // TLS defaults to require (hosted Postgres providers mandate it).
    // Respect ?sslmode=disable for local dev/test with docker Postgres.
    const tls: pg.Conn.Opts.TLS = if (hasSslModeDisable(query_string)) .off else .require;

    var host: []const u8 = hostport;
    var port: u16 = 5432;
    if (std.mem.lastIndexOfScalar(u8, hostport, ':')) |colon| {
        host = hostport[0..colon];
        port = std.fmt.parseInt(u16, hostport[colon + 1 ..], 10) catch return error.InvalidDatabaseUrl;
    }

    return pg.Pool.Opts{
        .size = 4,
        .connect = .{
            .host = try alloc.dupe(u8, host),
            .port = port,
            .tls = tls,
        },
        .auth = .{
            .username = try alloc.dupe(u8, username),
            .password = try alloc.dupe(u8, password),
            .database = try alloc.dupe(u8, dbname),
            .timeout = 10_000,
        },
    };
}

fn hasSslModeDisable(query: []const u8) bool {
    var it = std.mem.splitScalar(u8, query, '&');
    while (it.next()) |param| {
        if (std.mem.startsWith(u8, param, "sslmode=")) {
            const val = param["sslmode=".len..];
            if (std.mem.eql(u8, val, "disable")) return true;
        }
    }
    return false;
}

fn resolveDatabaseUrl(alloc: std.mem.Allocator, role: DbRole) ![]const u8 {
    const url = std.process.getEnvVarOwned(alloc, roleEnvVarName(role)) catch return error.MissingDatabaseUrl;
    if (std.mem.trim(u8, url, " \t\r\n").len == 0) {
        alloc.free(url);
        return error.MissingDatabaseUrl;
    }
    return url;
}

/// Initialize a pool using DATABASE_URL for the selected role.
pub fn initFromEnvForRole(alloc: std.mem.Allocator, role: DbRole) !*Pool {
    const url = resolveDatabaseUrl(alloc, role) catch {
        log.err("db.url_not_set role={s} error_code={s}", .{ @tagName(role), error_codes.ERR_INTERNAL_DB_UNAVAILABLE });
        return error.MissingDatabaseUrl;
    };
    defer alloc.free(url);

    // pg.Pool.init does NOT copy the connect/auth strings — they must remain
    // valid for the lifetime of the pool.  Use page_allocator so these
    // process-lifetime strings are not tracked by a GPA/arena and do not
    // appear as leaks when the process exits.
    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    log.info("db.pool_initialized role={s} size=4 host={s}", .{ @tagName(role), opts.connect.host orelse "127.0.0.1" });
    return pool;
}

/// Backward-compatible default initializer.
pub fn initFromEnv(alloc: std.mem.Allocator) !*Pool {
    return initFromEnvForRole(alloc, .default);
}

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
    var result = try conn.query(
        "SELECT 1 FROM audit.schema_migrations WHERE version = $1",
        .{version},
    );
    defer result.deinit();
    const applied = (try result.next()) != null;
    try result.drain();
    return applied;
}

fn hasFailedMigrationRecords(conn: *Conn) !bool {
    var result = try conn.query(
        "SELECT 1 FROM audit.schema_migration_failures LIMIT 1",
        .{},
    );
    defer result.deinit();
    const failed = (try result.next()) != null;
    try result.drain();
    return failed;
}

fn tryAcquireMigrationLock(conn: *Conn) !bool {
    var result = try conn.query("SELECT pg_try_advisory_lock($1)", .{MigrationAdvisoryLockKey});
    defer result.deinit();
    const row = try result.next() orelse return false;
    const acquired = try row.get(bool, 0);
    try result.drain();
    return acquired;
}

fn acquireMigrationLock(conn: *Conn) !void {
    var result = try conn.query("SELECT pg_advisory_lock($1)", .{MigrationAdvisoryLockKey});
    defer result.deinit();
    _ = try result.next();
    try result.drain();
}

fn releaseMigrationLock(conn: *Conn) void {
    var result = conn.query("SELECT pg_advisory_unlock($1)", .{MigrationAdvisoryLockKey}) catch return;
    defer result.deinit();
    _ = result.next() catch {};
    result.drain() catch {};
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

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = try conn.query("SELECT COALESCE(MAX(version), 0) FROM audit.schema_migrations", .{});
    defer result.deinit();
    const row = try result.next() orelse return 0;
    const version = try row.get(i32, 0);
    try result.drain();
    return version;
}

fn logPgErrorContext(conn: *Conn, op: []const u8) void {
    if (conn.err) |pg_err| {
        log.err("db.pg_error op={s} error_code={s} pg_code={s} message={s}", .{ op, error_codes.ERR_INTERNAL_DB_QUERY, pg_err.code, pg_err.message });
        if (pg_err.detail) |detail| {
            log.err("db.pg_error op={s} detail={s}", .{ op, detail });
        }
        if (pg_err.hint) |hint| {
            log.err("db.pg_error op={s} hint={s}", .{ op, hint });
        }
        return;
    }
    log.err("db.pg_error op={s} error_code={s} message=unknown", .{ op, error_codes.ERR_INTERNAL_DB_QUERY });
}

fn isUndefinedTablePgError(conn: *Conn) bool {
    if (conn.err) |pg_err| {
        return std.mem.eql(u8, pg_err.code, "42P01");
    }
    return false;
}

fn tableExists(conn: *Conn, query_sql: []const u8) !bool {
    var result = conn.query(query_sql, .{}) catch |err| {
        if (err == error.PG and isUndefinedTablePgError(conn)) return false;
        return err;
    };
    defer result.deinit();

    _ = result.next() catch |err| {
        if (err == error.PG and isUndefinedTablePgError(conn)) return false;
        return err;
    };

    result.drain() catch |err| {
        if (err == error.PG and isUndefinedTablePgError(conn)) return false;
        return err;
    };
    return true;
}

fn applySqlStatements(conn: *Conn, sql: []const u8) !u32 {
    var start: usize = 0;
    var i: usize = 0;
    var in_single_quote = false;
    var in_dollar_quote = false;
    var count: u32 = 0;

    while (i < sql.len) : (i += 1) {
        const ch = sql[i];

        // Skip -- line comments: advance past everything until newline.
        // Semicolons inside comments must not split statements.
        if (!in_single_quote and !in_dollar_quote and ch == '-' and i + 1 < sql.len and sql[i + 1] == '-') {
            while (i < sql.len and sql[i] != '\n') : (i += 1) {}
            continue;
        }

        if (!in_dollar_quote and ch == '\'') {
            // Skip escaped single quote inside string literal.
            if (in_single_quote and i + 1 < sql.len and sql[i + 1] == '\'') {
                i += 1;
                continue;
            }
            in_single_quote = !in_single_quote;
            continue;
        }

        // NOTE: Only bare $$ dollar-quoting is supported.
        // Tagged dollar-quotes (e.g. $body$...$body$) are not handled.
        // All schema files must use plain $$ — do not use tagged variants.
        if (!in_single_quote and i + 1 < sql.len and sql[i] == '$' and sql[i + 1] == '$') {
            in_dollar_quote = !in_dollar_quote;
            i += 1;
            continue;
        }

        if (ch != ';' or in_single_quote or in_dollar_quote) continue;

        const stmt = std.mem.trim(u8, sql[start..i], " \t\r\n");
        if (stmt.len > 0) {
            const preview_len = @min(stmt.len, 120);
            log.debug("migrate.stmt index={d} preview={s}", .{ count + 1, stmt[0..preview_len] });
            _ = try conn.exec(stmt, .{});
            count += 1;
        }

        start = i + 1;
    }

    const tail = std.mem.trim(u8, sql[start..], " \t\r\n");
    if (tail.len > 0) {
        const preview_len = @min(tail.len, 120);
        log.debug("migrate.stmt index={d} preview={s}", .{ count + 1, tail[0..preview_len] });
        _ = try conn.exec(tail, .{});
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
    _ = conn.exec("ROLLBACK", .{}) catch {};
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
        log.info("db.migration_applied version={d} statements={d}", .{ migration.version, statements });
    }
}

test "hasSslModeDisable detects disable in query string" {
    try std.testing.expect(hasSslModeDisable("sslmode=disable"));
    try std.testing.expect(hasSslModeDisable("application_name=test&sslmode=disable"));
    try std.testing.expect(hasSslModeDisable("sslmode=disable&timeout=10"));
    try std.testing.expect(!hasSslModeDisable("sslmode=require"));
    try std.testing.expect(!hasSslModeDisable("sslmode=verify-full"));
    try std.testing.expect(!hasSslModeDisable(""));
    try std.testing.expect(!hasSslModeDisable("application_name=test"));
}

test {
    _ = @import("./pool_test.zig");
}
