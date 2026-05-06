//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.
//!
//! Migration plumbing (versioned schema runner + state inspector) lives in
//! `pool_migrations.zig` and is re-exported here so callers continue using
//! `pool.runMigrations(...)` / `pool.inspectMigrationState(...)`.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");
const pool_migrations = @import("pool_migrations.zig");

const log = logging.scoped(.db);

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

pub const inspectMigrationState = pool_migrations.inspectMigrationState;
pub const runMigrations = pool_migrations.runMigrations;

/// Parse a Postgres connection URL into pg.Pool.Opts.
/// URL format: postgres://user:pass@host:port/dbname[?query]
/// TLS is always required — all role-separated connections go to hosted Postgres
/// providers (PlanetScale, Neon, Supabase) that mandate TLS.
pub fn parseUrl(alloc: std.mem.Allocator, url: []const u8) !pg.Pool.Opts {
    const rest = if (std.mem.startsWith(u8, url, "postgres://"))
        url["postgres://".len..]
    else if (std.mem.startsWith(u8, url, "postgresql://"))
        url["postgresql://".len..]
    else
        return error.InvalidDatabaseUrl;

    const at_pos = std.mem.lastIndexOfScalar(u8, rest, '@') orelse return error.InvalidDatabaseUrl;
    const userpass = rest[0..at_pos];
    const hostpath = rest[at_pos + 1 ..];

    var username: []const u8 = "";
    var password: []const u8 = "";
    if (std.mem.indexOfScalar(u8, userpass, ':')) |colon| {
        username = userpass[0..colon];
        password = userpass[colon + 1 ..];
    } else {
        username = userpass;
    }

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
        log.err("url_not_set", .{ .role = @tagName(role), .error_code = error_codes.ERR_INTERNAL_DB_UNAVAILABLE });
        return error.MissingDatabaseUrl;
    };
    defer alloc.free(url);

    // pg.Pool.init does NOT copy the connect/auth strings — they must remain
    // valid for the lifetime of the pool. Use page_allocator so these
    // process-lifetime strings are not tracked by a GPA/arena and do not
    // appear as leaks when the process exits.
    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    log.info("pool_initialized", .{ .role = @tagName(role), .size = 4, .host = opts.connect.host orelse "127.0.0.1" });
    return pool;
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
