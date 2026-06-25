//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.
//!
//! Migration plumbing (versioned schema runner + state inspector) lives in
//! `pool_migrations.zig` and is re-exported here so callers continue using
//! `pool.runMigrations(...)` / `pool.inspectMigrationState(...)`.

const std = @import("std");
const common = @import("common");
const URL_TRIM_CHARS = " \t\r\n";
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");
const pool_migrations = @import("pool_migrations.zig");
const env_resolve = @import("../config/env_resolve.zig");
const pool_types = @import("pool_types.zig");

const EnvMap = common.env.Map;

const log = logging.scoped(.db);

pub const Pool = pg.Pool;
pub const Conn = pg.Conn;
pub const Row = pg.Row;

const S_SSLMODE = "sslmode=";

// Pool sizing + acquire-timeout knobs (env-tunable, role-aware).
//
// `DATABASE_POOL_SIZE` / `DATABASE_ACQUIRE_TIMEOUT_MS` apply to every role;
// a role-prefixed override (`DATABASE_POOL_SIZE_API`, ...) wins when present.
const POOL_SIZE_ENV = "DATABASE_POOL_SIZE";
const ACQUIRE_TIMEOUT_MS_ENV = "DATABASE_ACQUIRE_TIMEOUT_MS";

// Default pool size is a small fraction of the API in-flight-request ceiling:
// many concurrent requests share a handful of DB connections, so the pool need
// not scale 1:1 with request concurrency. Mirrors the `API_MAX_IN_FLIGHT_REQUESTS`
// loader default (256) divided by the per-connection request-sharing factor.
const API_MAX_IN_FLIGHT_REQUESTS_DEFAULT: u16 = 256;
const POOL_SIZE_INFLIGHT_DIVISOR: u16 = 64;
const POOL_SIZE_DEFAULT: u16 = API_MAX_IN_FLIGHT_REQUESTS_DEFAULT / POOL_SIZE_INFLIGHT_DIVISOR;

// Acquire timeout fails fast: a starved pool surfaces as a quick error rather
// than a multi-second stall that masquerades as a slow request.
const ACQUIRE_TIMEOUT_MS_DEFAULT: u32 = 2_000;

// Connection (auth handshake) timeout — distinct from the pool acquire timeout.
const CONNECT_TIMEOUT_MS_DEFAULT: u32 = 10_000;

// Upper bound on a role tag ("migrator" is the longest) and on a fully
// composed "<KNOB>_<ROLE>" env-var name; both leave slack for future roles.
const ROLE_TAG_MAX = 16;
const ROLE_ENV_NAME_MAX = 64;

pub const DbRole = enum {
    default,
    api,
    callback,
    migrator,
};

pub fn roleEnvVarName(role: DbRole) []const u8 {
    return switch (role) {
        .api => "DATABASE_URL_API",
        .callback => "DATABASE_URL_CALLBACK",
        .migrator => "DATABASE_URL_MIGRATOR",
        .default => "DATABASE_URL",
    };
}

pub const Migration = pool_types.Migration;
pub const MigrationState = pool_types.MigrationState;

pub const inspectMigrationState = pool_migrations.inspectMigrationState;
pub const runMigrations = pool_migrations.runMigrations;
pub const assertMigratorSessionConnection = pool_migrations.assertMigratorSessionConnection;

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

    // `.size` / `.timeout` (pool acquire timeout) default here and are
    // overwritten from env-resolved sizing (resolveSizing) in initFromEnvForRole.
    return pg.Pool.Opts{
        .size = POOL_SIZE_DEFAULT,
        .timeout = ACQUIRE_TIMEOUT_MS_DEFAULT,
        .connect = .{
            .host = try alloc.dupe(u8, host),
            .port = port,
            .tls = tls,
        },
        .auth = .{
            .username = try alloc.dupe(u8, username),
            .password = try alloc.dupe(u8, password),
            .database = try alloc.dupe(u8, dbname),
            .timeout = CONNECT_TIMEOUT_MS_DEFAULT,
        },
    };
}

fn hasSslModeDisable(query: []const u8) bool {
    var it = std.mem.splitScalar(u8, query, '&');
    while (it.next()) |param| {
        if (std.mem.startsWith(u8, param, S_SSLMODE)) {
            const val = param[S_SSLMODE.len..];
            if (std.mem.eql(u8, val, "disable")) return true;
        }
    }
    return false;
}

fn resolveDatabaseUrl(env_map: *const EnvMap, alloc: std.mem.Allocator, role: DbRole) ![]const u8 {
    const url = (try common.env.owned(env_map, alloc, roleEnvVarName(role))) orelse return error.MissingDatabaseUrl;
    if (std.mem.trim(u8, url, URL_TRIM_CHARS).len == 0) {
        alloc.free(url);
        return error.MissingDatabaseUrl;
    }
    return url;
}

/// Read a u32 env knob, preferring the role-prefixed override
/// ("<base>_<ROLE>") over the generic `base`. Absent/blank → `default_value`;
/// present-but-malformed → `default_value` (caller logs the fallback).
fn readRoleEnvU32(env_map: *const EnvMap, alloc: std.mem.Allocator, base: []const u8, role: DbRole, default_value: u32) u32 {
    var role_buf: [ROLE_TAG_MAX]u8 = undefined;
    const role_upper = std.ascii.upperString(&role_buf, @tagName(role));

    var name_buf: [ROLE_ENV_NAME_MAX]u8 = undefined;
    const scoped = std.fmt.bufPrint(&name_buf, "{s}_{s}", .{ base, role_upper }) catch base;
    if (parseEnvU32(env_map, alloc, scoped)) |v| return v;
    if (parseEnvU32(env_map, alloc, base)) |v| return v;
    return default_value;
}

/// Parse a non-empty u32 from a raw env value; null when blank or unparseable.
fn parseSizeStr(raw: []const u8) ?u32 {
    const trimmed = std.mem.trim(u8, raw, URL_TRIM_CHARS);
    if (trimmed.len == 0) return null;
    return std.fmt.parseInt(u32, trimmed, 10) catch null;
}

/// Parse a non-empty u32 env var; null when unset, blank, or unparseable.
fn parseEnvU32(env_map: *const EnvMap, alloc: std.mem.Allocator, name: []const u8) ?u32 {
    const raw = env_resolve.config(env_map, alloc, name) orelse return null;
    defer alloc.free(raw);
    return parseSizeStr(raw);
}

/// Clamp a raw pool size into the u16 connection-count domain; 0 or out-of-range
/// falls back to the default (never a 0-connection pool).
fn clampPoolSize(raw: u32) u16 {
    if (raw == 0 or raw > std.math.maxInt(u16)) return POOL_SIZE_DEFAULT;
    return @intCast(raw);
}

/// Resolve env-tunable pool sizing for a role, clamping pool size to the
/// u16 connection-count domain. Defaults apply when env knobs are absent.
fn resolveSizing(env_map: *const EnvMap, alloc: std.mem.Allocator, role: DbRole) struct { size: u16, timeout_ms: u32 } {
    const size_raw = readRoleEnvU32(env_map, alloc, POOL_SIZE_ENV, role, POOL_SIZE_DEFAULT);
    const timeout_ms = readRoleEnvU32(env_map, alloc, ACQUIRE_TIMEOUT_MS_ENV, role, ACQUIRE_TIMEOUT_MS_DEFAULT);
    return .{ .size = clampPoolSize(size_raw), .timeout_ms = timeout_ms };
}

/// Initialize a pool using DATABASE_URL for the selected role. `io` backs the
/// pg connection/retry loop (Zig 0.16 `pg.Pool.init` takes `Io` first).
pub fn initFromEnvForRole(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, role: DbRole) !*Pool {
    const url = resolveDatabaseUrl(env_map, alloc, role) catch {
        log.err("url_not_set", .{ .role = @tagName(role), .error_code = error_codes.ERR_INTERNAL_DB_UNAVAILABLE });
        return error.MissingDatabaseUrl;
    };
    defer alloc.free(url);

    // pg.Pool.init does NOT copy the connect/auth strings — they must remain
    // valid for the lifetime of the pool. Use page_allocator so these
    // process-lifetime strings are not tracked by a GPA/arena and do not
    // appear as leaks when the process exits.
    var opts = try parseUrl(std.heap.page_allocator, url);
    const sizing = resolveSizing(env_map, alloc, role);
    opts.size = sizing.size;
    opts.timeout = sizing.timeout_ms;
    const pool = try pg.Pool.init(io, alloc, opts);
    log.info("pool_initialized", .{
        .role = @tagName(role),
        .size = opts.size,
        .acquire_timeout_ms = opts.timeout,
        .host = opts.connect.host orelse "127.0.0.1",
    });
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

test "parseSizeStr accepts a clean u32 and rejects blank/garbage" {
    try std.testing.expectEqual(@as(?u32, 12), parseSizeStr("12"));
    try std.testing.expectEqual(@as(?u32, 750), parseSizeStr("  750\n")); // trims surrounding ws
    try std.testing.expectEqual(@as(?u32, null), parseSizeStr(""));
    try std.testing.expectEqual(@as(?u32, null), parseSizeStr("   "));
    try std.testing.expectEqual(@as(?u32, null), parseSizeStr("not-a-number"));
}

test "clampPoolSize keeps in-range sizes and floors invalid ones to the default" {
    try std.testing.expectEqual(@as(u16, 12), clampPoolSize(12));
    try std.testing.expectEqual(@as(u16, 1), clampPoolSize(1));
    try std.testing.expectEqual(POOL_SIZE_DEFAULT, clampPoolSize(0)); // never a 0-conn pool
    try std.testing.expectEqual(POOL_SIZE_DEFAULT, clampPoolSize(std.math.maxInt(u32))); // out of u16 range
    try std.testing.expectEqual(@as(u16, std.math.maxInt(u16)), clampPoolSize(std.math.maxInt(u16)));
}

test {
    _ = @import("./pool_test.zig");
}
