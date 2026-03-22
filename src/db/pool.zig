//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.

const std = @import("std");
const pg = @import("pg");
const id_format = @import("../types/id_format.zig");
const log = std.log.scoped(.db);

pub const Pool = pg.Pool;
pub const Conn = pg.Conn;
pub const Row = pg.Row;

pub const DbRole = enum {
    default,
    api,
    worker,
    callback,
};

pub fn roleEnvVarName(role: DbRole) []const u8 {
    return switch (role) {
        .api => "DATABASE_URL_API",
        .worker => "DATABASE_URL_WORKER",
        .callback => "DATABASE_URL_CALLBACK",
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
        log.err("db.url_not_set role={s} error_code=UZ-INTERNAL-001", .{@tagName(role)});
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

fn ensureSchemaMigrationsTable(conn: *Conn) !void {
    var result = try conn.query(
        \\CREATE TABLE IF NOT EXISTS schema_migrations (
        \\    version     INTEGER PRIMARY KEY,
        \\    applied_at  BIGINT NOT NULL
        \\)
    , .{});
    defer result.deinit();
    try result.drain();
}

fn ensureSchemaMigrationFailuresTable(conn: *Conn) !void {
    var result = try conn.query(
        \\CREATE TABLE IF NOT EXISTS schema_migration_failures (
        \\    version     INTEGER PRIMARY KEY,
        \\    failed_at   BIGINT NOT NULL,
        \\    error_text  TEXT NOT NULL
        \\)
    , .{});
    defer result.deinit();
    try result.drain();
}

fn isMigrationApplied(conn: *Conn, version: i32) !bool {
    var result = try conn.query(
        "SELECT 1 FROM schema_migrations WHERE version = $1",
        .{version},
    );
    defer result.deinit();
    const applied = (try result.next()) != null;
    try result.drain();
    return applied;
}

fn hasFailedMigrationRecords(conn: *Conn) !bool {
    var result = try conn.query(
        "SELECT 1 FROM schema_migration_failures LIMIT 1",
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
    var q = conn.query(
        \\INSERT INTO schema_migration_failures (version, failed_at, error_text)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (version) DO UPDATE
        \\SET failed_at = EXCLUDED.failed_at,
        \\    error_text = EXCLUDED.error_text
    , .{ version, ts, @errorName(err) }) catch return;
    defer q.deinit();
    q.drain() catch {};
}

fn clearMigrationFailure(conn: *Conn, version: i32) void {
    var q = conn.query("DELETE FROM schema_migration_failures WHERE version = $1", .{version}) catch return;
    defer q.deinit();
    q.drain() catch {};
}

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = try conn.query("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", .{});
    defer result.deinit();
    const row = try result.next() orelse return 0;
    const version = try row.get(i32, 0);
    try result.drain();
    return version;
}

fn applySqlStatements(conn: *Conn, sql: []const u8) !u32 {
    var start: usize = 0;
    var i: usize = 0;
    var in_single_quote = false;
    var in_dollar_quote = false;
    var count: u32 = 0;

    while (i < sql.len) : (i += 1) {
        const ch = sql[i];

        if (!in_dollar_quote and ch == '\'') {
            // Skip escaped single quote inside string literal.
            if (in_single_quote and i + 1 < sql.len and sql[i + 1] == '\'') {
                i += 1;
                continue;
            }
            in_single_quote = !in_single_quote;
            continue;
        }

        if (!in_single_quote and i + 1 < sql.len and sql[i] == '$' and sql[i + 1] == '$') {
            in_dollar_quote = !in_dollar_quote;
            i += 1;
            continue;
        }

        if (ch != ';' or in_single_quote or in_dollar_quote) continue;

        const stmt = std.mem.trim(u8, sql[start..i], " \t\r\n");
        if (stmt.len > 0) {
            var result = try conn.query(stmt, .{});
            defer result.deinit();
            try result.drain();
            count += 1;
        }

        start = i + 1;
    }

    const tail = std.mem.trim(u8, sql[start..], " \t\r\n");
    if (tail.len > 0) {
        var result = try conn.query(tail, .{});
        defer result.deinit();
        try result.drain();
        count += 1;
    }

    return count;
}

fn beginTx(conn: *Conn) !void {
    var tx = try conn.query("BEGIN", .{});
    defer tx.deinit();
    try tx.drain();
}

fn commitTx(conn: *Conn) !void {
    var tx = try conn.query("COMMIT", .{});
    defer tx.deinit();
    try tx.drain();
}

fn rollbackTx(conn: *Conn) void {
    var tx = conn.query("ROLLBACK", .{}) catch return;
    defer tx.deinit();
    tx.drain() catch {};
}

pub fn inspectMigrationState(pool: *Pool, migrations: []const Migration) !MigrationState {
    const conn = try pool.acquire();
    defer pool.release(conn);

    try ensureSchemaMigrationsTable(conn);
    try ensureSchemaMigrationFailuresTable(conn);

    var applied_versions: u32 = 0;
    var latest_expected: i32 = 0;
    for (migrations) |migration| {
        latest_expected = @max(latest_expected, migration.version);
        if (try isMigrationApplied(conn, migration.version)) {
            applied_versions += 1;
        }
    }

    const latest_applied = try maxAppliedMigrationVersion(conn);
    const failed = try hasFailedMigrationRecords(conn);
    const lock_available = try tryAcquireMigrationLock(conn);
    if (lock_available) releaseMigrationLock(conn);

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

    try ensureSchemaMigrationsTable(conn);
    try ensureSchemaMigrationFailuresTable(conn);

    try acquireMigrationLock(conn);
    defer releaseMigrationLock(conn);

    for (migrations) |migration| {
        if (try isMigrationApplied(conn, migration.version)) {
            clearMigrationFailure(conn, migration.version);
            continue;
        }

        try beginTx(conn);
        const statements = applySqlStatements(conn, migration.sql) catch |err| {
            rollbackTx(conn);
            markMigrationFailure(conn, migration.version, err);
            return err;
        };

        var insert = try conn.query(
            "INSERT INTO schema_migrations (version, applied_at) VALUES ($1, $2)",
            .{ migration.version, std.time.milliTimestamp() },
        );
        defer insert.deinit();
        try insert.drain();

        commitTx(conn) catch |err| {
            rollbackTx(conn);
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
