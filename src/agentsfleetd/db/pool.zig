//! Database connection pool — wraps pg.zig Pool.
//! Owns the pool and provides helpers for common queries.
//!
//! Migration plumbing is split between `pool_migrations.zig` (runner) and
//! `pool_migration_state.zig` (inspector), then re-exported through
//! `pool_migrations.zig` so callers keep using the `pool` entry points.

const std = @import("std");
const common = @import("common");
const URL_TRIM_CHARS = " \t\r\n";
const pg = @import("pg");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");
const pool_migrations = @import("pool_migrations.zig");
const pool_elevation = @import("pool_elevation.zig");
const env_resolve = @import("../config/env_resolve.zig");
const pool_types = @import("pool_types.zig");
const pool_url = @import("pool_url.zig");

const EnvMap = common.env.Map;

const log = logging.scoped(.db);

pub const Conn = pg.Conn;

// URL parsing lives in its own module (RULE FLL); the alias keeps `db.parseUrl`
// the one spelling every caller uses.
pub const parseUrl = pool_url.parseUrl;

/// The repository-owned pool: pg.Pool plus the one invariant the vendored pool
/// cannot state — a connection is never pooled while elevated (spec §3 of the
/// privilege boundary). Same `acquire`/`release` shape as pg.Pool, so borrowers
/// are unchanged; release is the single choke point every borrower passes
/// through, which is why the guard lives here and not at call sites.
pub const Pool = struct {
    inner: *pg.Pool,
    alloc: std.mem.Allocator,

    pub fn acquire(self: *Pool) !*Conn {
        return self.inner.acquire();
    }

    /// Release with the elevation backstop. A connection whose elevation scope
    /// never ended is refused: reported under `UZ-INTERNAL-005` (inside
    /// `auditRelease`), counted, and handed to pg's dirty path — `begin()`
    /// moves an idle connection off `.idle`, and the vendored release destroys
    /// and replaces any non-idle connection rather than pooling it. A failed
    /// `begin` leaves the connection in `.fail`, which the same path destroys.
    pub fn release(self: *Pool, conn: *Conn) void {
        if (pool_elevation.auditRelease(conn) != null) {
            if (conn._state == .idle) {
                conn.begin() catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
            }
        }
        self.inner.release(conn);
    }

    pub fn deinit(self: *Pool) void {
        const alloc = self.alloc;
        self.inner.deinit();
        self.* = undefined;
        alloc.destroy(self);
    }
};

// Pool sizing + acquire-timeout knobs (env-tunable, role-aware).
//
// `DATABASE_POOL_SIZE` / `DATABASE_ACQUIRE_TIMEOUT_MS` apply to every role;
// a role-prefixed override (`DATABASE_POOL_SIZE_API`, ...) wins when present.
const POOL_SIZE_ENV = "DATABASE_POOL_SIZE";
const ACQUIRE_TIMEOUT_MS_ENV = "DATABASE_ACQUIRE_TIMEOUT_MS";

// Pool sizing lives in `pool_url.zig` — one definition, imported here, so the
// two files cannot drift on the value that decides how many connections a
// deployment opens.
const POOL_SIZE_DEFAULT = pool_url.POOL_SIZE_DEFAULT;
const ACQUIRE_TIMEOUT_MS_DEFAULT = pool_url.ACQUIRE_TIMEOUT_MS_DEFAULT;

// Upper bound on a role tag ("migrator" is the longest) and on a fully
// composed "<KNOB>_<ROLE>" env-var name; both leave slack for future roles.
const ROLE_TAG_MAX = 16;
const ROLE_ENV_NAME_MAX = 64;

pub const DbRole = enum {
    default,
    api,
    migrator,
};

pub fn roleEnvVarName(role: DbRole) []const u8 {
    return switch (role) {
        .api => "DATABASE_URL_API",
        .migrator => "DATABASE_URL_MIGRATOR",
        .default => "DATABASE_URL",
    };
}

pub const Migration = pool_types.Migration;
pub const MigrationState = pool_types.MigrationState;

// Migration entry points delegate to the raw pg pool: migrations run as
// db_migrator (or the local superuser) and never elevate, so the release
// backstop has nothing to audit on that path.
pub fn inspectMigrationState(pool: *Pool, migrations: []const Migration) !MigrationState {
    return pool_migrations.inspectMigrationState(pool.inner, migrations);
}

pub fn runMigrations(pool: *Pool, migrations: []const Migration) !void {
    return pool_migrations.runMigrations(pool.inner, migrations);
}

pub fn runMigrationsRefusingNewer(pool: *Pool, migrations: []const Migration) !void {
    return pool_migrations.runMigrationsRefusingNewer(pool.inner, migrations);
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

/// Wrap a raw pg pool in the repository Pool. The wrapper owns `inner` from
/// here on — `deinit` tears down both. This is the one construction seam, used
/// by `initFromEnvForRole` and by tests that build their pg pool directly.
pub fn adopt(inner: *pg.Pool, alloc: std.mem.Allocator) !*Pool {
    const pool = try alloc.create(Pool);
    pool.* = .{ .inner = inner, .alloc = alloc };
    return pool;
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
    const inner = try pg.Pool.init(io, alloc, opts);
    errdefer inner.deinit();
    const pool = try adopt(inner, alloc);
    log.info("pool_initialized", .{
        .role = @tagName(role),
        .size = opts.size,
        .acquire_timeout_ms = opts.timeout,
        .host = opts.connect.host orelse "127.0.0.1",
    });
    return pool;
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
    _ = @import("./pool_url.zig");
}
