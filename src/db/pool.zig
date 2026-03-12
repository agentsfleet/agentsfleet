//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.

const std = @import("std");
const pg = @import("pg");
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
/// URL format: postgres://user:pass@host:port/dbname
pub fn parseUrl(alloc: std.mem.Allocator, url: []const u8) !pg.Pool.Opts {
    // strip scheme
    const rest = if (std.mem.startsWith(u8, url, "postgres://"))
        url["postgres://".len..]
    else if (std.mem.startsWith(u8, url, "postgresql://"))
        url["postgresql://".len..]
    else
        return error.InvalidDatabaseUrl;

    // user:pass@host:port/dbname
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

    // host:port/dbname
    const slash_pos = std.mem.indexOfScalar(u8, hostpath, '/') orelse return error.InvalidDatabaseUrl;
    const hostport = hostpath[0..slash_pos];
    const dbname = hostpath[slash_pos + 1 ..];

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
        },
        .auth = .{
            .username = try alloc.dupe(u8, username),
            .password = try alloc.dupe(u8, password),
            .database = try alloc.dupe(u8, dbname),
            .timeout = 10_000,
        },
    };
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
        log.err("database url not set for role={s}", .{@tagName(role)});
        return error.MissingDatabaseUrl;
    };
    defer alloc.free(url);

    // Use an arena for the URL-parsed opts so pg.Pool.init can copy them
    // and we can free everything cleanly afterward.
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    const opts = try parseUrl(arena.allocator(), url);
    const host_copy = try alloc.dupe(u8, opts.connect.host orelse "127.0.0.1");
    defer alloc.free(host_copy);

    const pool = try pg.Pool.init(alloc, opts);
    log.info("database pool initialized role={s} size=4 host={s}", .{ @tagName(role), host_copy });
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
    result.deinit();
}

fn ensureSchemaMigrationFailuresTable(conn: *Conn) !void {
    var result = try conn.query(
        \\CREATE TABLE IF NOT EXISTS schema_migration_failures (
        \\    version     INTEGER PRIMARY KEY,
        \\    failed_at   BIGINT NOT NULL,
        \\    error_text  TEXT NOT NULL
        \\)
    , .{});
    result.deinit();
}

fn isMigrationApplied(conn: *Conn, version: i32) !bool {
    var result = try conn.query(
        "SELECT 1 FROM schema_migrations WHERE version = $1",
        .{version},
    );
    defer result.deinit();
    return (try result.next()) != null;
}

fn hasFailedMigrationRecords(conn: *Conn) !bool {
    var result = try conn.query(
        "SELECT 1 FROM schema_migration_failures LIMIT 1",
        .{},
    );
    defer result.deinit();
    return (try result.next()) != null;
}

fn tryAcquireMigrationLock(conn: *Conn) !bool {
    var result = try conn.query("SELECT pg_try_advisory_lock($1)", .{MigrationAdvisoryLockKey});
    defer result.deinit();
    const row = try result.next() orelse return false;
    return try row.get(bool, 0);
}

fn acquireMigrationLock(conn: *Conn) !void {
    var result = try conn.query("SELECT pg_advisory_lock($1)", .{MigrationAdvisoryLockKey});
    result.deinit();
}

fn releaseMigrationLock(conn: *Conn) void {
    var result = conn.query("SELECT pg_advisory_unlock($1)", .{MigrationAdvisoryLockKey}) catch return;
    result.deinit();
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
    q.deinit();
}

fn clearMigrationFailure(conn: *Conn, version: i32) void {
    var q = conn.query("DELETE FROM schema_migration_failures WHERE version = $1", .{version}) catch return;
    q.deinit();
}

fn maxAppliedMigrationVersion(conn: *Conn) !i32 {
    var result = try conn.query("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", .{});
    defer result.deinit();
    const row = try result.next() orelse return 0;
    return try row.get(i32, 0);
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
            result.deinit();
            count += 1;
        }

        start = i + 1;
    }

    const tail = std.mem.trim(u8, sql[start..], " \t\r\n");
    if (tail.len > 0) {
        var result = try conn.query(tail, .{});
        result.deinit();
        count += 1;
    }

    return count;
}

fn beginTx(conn: *Conn) !void {
    var tx = try conn.query("BEGIN", .{});
    tx.deinit();
}

fn commitTx(conn: *Conn) !void {
    var tx = try conn.query("COMMIT", .{});
    tx.deinit();
}

fn rollbackTx(conn: *Conn) void {
    var tx = conn.query("ROLLBACK", .{}) catch return;
    tx.deinit();
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
        insert.deinit();

        commitTx(conn) catch |err| {
            rollbackTx(conn);
            markMigrationFailure(conn, migration.version, err);
            return err;
        };
        clearMigrationFailure(conn, migration.version);
        log.info("migration applied version={d} statements={d}", .{ migration.version, statements });
    }
}

test "parseUrl parses host, port, db, credentials" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://alice:secret@localhost:5433/usezombiedb");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    const password = opts.auth.password.?;
    defer alloc.free(password);
    const database = opts.auth.database.?;
    defer alloc.free(database);

    try std.testing.expectEqualStrings("localhost", opts.connect.host.?);
    try std.testing.expectEqual(@as(u16, 5433), opts.connect.port.?);
    try std.testing.expectEqualStrings("alice", opts.auth.username);
    try std.testing.expectEqualStrings("secret", password);
    try std.testing.expectEqualStrings("usezombiedb", database);
}

test "roleEnvVarName maps db roles deterministically" {
    try std.testing.expectEqualStrings("DATABASE_URL", roleEnvVarName(.default));
    try std.testing.expectEqualStrings("DATABASE_URL_API", roleEnvVarName(.api));
    try std.testing.expectEqualStrings("DATABASE_URL_WORKER", roleEnvVarName(.worker));
    try std.testing.expectEqualStrings("DATABASE_URL_CALLBACK", roleEnvVarName(.callback));
}

fn openIntegrationTestConn(alloc: std.mem.Allocator) !?struct { pool: *Pool, conn: *Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try parseUrl(arena.allocator(), url);
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

fn createUuidMigrationTempSchema(conn: *Conn) !void {
    var q = try conn.query(
        \\CREATE TEMP TABLE runs (
        \\  run_id TEXT PRIMARY KEY,
        \\  run_snapshot_version TEXT
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE profile_compile_jobs (
        \\  compile_job_id TEXT PRIMARY KEY
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_transitions (
        \\  run_id TEXT NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE artifacts (
        \\  run_id TEXT NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE usage_ledger (
        \\  run_id TEXT NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_side_effects (
        \\  run_id TEXT NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_side_effect_outbox (
        \\  run_id TEXT NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE workspace_memories (
        \\  run_id TEXT NOT NULL
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE policy_events (
        \\  run_id TEXT
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE workspace_active_profile (
        \\  profile_version_id TEXT NOT NULL REFERENCES agent_profile_versions(profile_version_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE profile_linkage_audit_artifacts (
        \\  profile_version_id TEXT NOT NULL REFERENCES agent_profile_versions(profile_version_id),
        \\  compile_job_id TEXT REFERENCES profile_compile_jobs(compile_job_id),
        \\  run_id TEXT REFERENCES runs(run_id)
        \\) ON COMMIT DROP
    , .{});
    q.deinit();
}

test "integration: uuid migration converts core id columns to uuid and blocks rollback when rows exist" {
    if (!std.process.hasEnvVarConstant("UUID_MIGRATION_TESTS")) return error.SkipZigTest;
    const db_ctx = (try openIntegrationTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createUuidMigrationTempSchema(db_ctx.conn);

    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    const pver_id = "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98";
    const cjob_id = "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97";

    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO runs (run_id, run_snapshot_version) VALUES ($1, $2);
            \\INSERT INTO agent_profile_versions (profile_version_id) VALUES ($2);
            \\INSERT INTO profile_compile_jobs (compile_job_id) VALUES ($3);
            \\INSERT INTO run_transitions (run_id) VALUES ($1);
            \\INSERT INTO artifacts (run_id) VALUES ($1);
            \\INSERT INTO usage_ledger (run_id) VALUES ($1);
            \\INSERT INTO run_side_effects (run_id) VALUES ($1);
            \\INSERT INTO run_side_effect_outbox (run_id) VALUES ($1);
            \\INSERT INTO workspace_memories (run_id) VALUES ($1);
            \\INSERT INTO policy_events (run_id) VALUES ($1);
            \\INSERT INTO workspace_active_profile (profile_version_id) VALUES ($2);
            \\INSERT INTO profile_linkage_audit_artifacts (profile_version_id, compile_job_id, run_id) VALUES ($2, $3, $1)
        , .{ run_id, pver_id, cjob_id });
        q.deinit();
    }

    const schema = @import("schema");
    _ = try applySqlStatements(db_ctx.conn, schema.uuidv7_id_migration_sql);

    {
        var q = try db_ctx.conn.query(
            \\SELECT data_type
            \\FROM information_schema.columns
            \\WHERE table_name = 'runs' AND column_name = 'run_id'
        , .{});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("uuid", try row.get([]const u8, 0));
    }

    try std.testing.expectError(error.PgError, db_ctx.conn.query(
        "SELECT assert_uuidv7_rollback_allowed()",
        .{},
    ));
}

test "integration: uuid migration fails fast when legacy prefixed ids still exist" {
    if (!std.process.hasEnvVarConstant("UUID_MIGRATION_TESTS")) return error.SkipZigTest;
    const db_ctx = (try openIntegrationTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createUuidMigrationTempSchema(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO runs (run_id, run_snapshot_version) VALUES ('run_legacy', NULL);
            \\INSERT INTO workspace_memories (run_id) VALUES ('run_legacy')
        , .{});
        q.deinit();
    }

    const schema = @import("schema");
    try std.testing.expectError(error.PgError, applySqlStatements(db_ctx.conn, schema.uuidv7_id_migration_sql));
}
