const std = @import("std");

const db = @import("../db/pool.zig");
const queue_redis = @import("../queue/redis.zig");

const S_T_R_N = " \t\r\n";
const PANIC_OOM = "OOM";

pub const EnvVarsErrors = error{
    MissingDatabaseUrlApi,
    MissingRedisUrlApi,
    RedisApiTlsRequired,
};

const EnvVars = struct {
    db_api: ?[]u8,
    redis_api: ?[]u8,
    alloc: std.mem.Allocator,

    pub fn deinit(self: *EnvVars) void {
        if (self.db_api) |v| self.alloc.free(v);
        if (self.redis_api) |v| self.alloc.free(v);
    }
};

pub fn loadFromEnv(alloc: std.mem.Allocator) EnvVars {
    return .{
        .db_api = std.process.getEnvVarOwned(alloc, db.roleEnvVarName(.api)) catch null,
        .redis_api = std.process.getEnvVarOwned(alloc, queue_redis.roleEnvVarName(.api)) catch null,
        .alloc = alloc,
    };
}

pub fn validateLoaded(urls: EnvVars) EnvVarsErrors!void {
    const db_api = urls.db_api orelse return EnvVarsErrors.MissingDatabaseUrlApi;
    const redis_api = urls.redis_api orelse return EnvVarsErrors.MissingRedisUrlApi;
    if (std.mem.trim(u8, db_api, S_T_R_N).len == 0) return EnvVarsErrors.MissingDatabaseUrlApi;
    if (std.mem.trim(u8, redis_api, S_T_R_N).len == 0) return EnvVarsErrors.MissingRedisUrlApi;
    if (!std.mem.startsWith(u8, redis_api, "rediss://")) return EnvVarsErrors.RedisApiTlsRequired;
}

pub fn enforceFromEnv(alloc: std.mem.Allocator) EnvVarsErrors!void {
    var urls = loadFromEnv(alloc);
    defer urls.deinit();
    try validateLoaded(urls);
}

// --- API-only env validation tests ---

fn testEnvVars(db_api: ?[]const u8, redis_api: ?[]const u8) EnvVars {
    const alloc = std.testing.allocator;
    return .{
        .db_api = if (db_api) |s| alloc.dupe(u8, s) catch @panic(PANIC_OOM) else null,
        .redis_api = if (redis_api) |s| alloc.dupe(u8, s) catch @panic(PANIC_OOM) else null,
        .alloc = alloc,
    };
}

test "validateLoaded rejects missing API DB URL" {
    var urls = testEnvVars(null, "rediss://api:pw@cache.local:6379");
    defer urls.deinit();
    try std.testing.expectError(EnvVarsErrors.MissingDatabaseUrlApi, validateLoaded(urls));
}

test "validateLoaded rejects whitespace-only API DB URL" {
    var urls = testEnvVars("  \t\n", "rediss://api:pw@cache.local:6379");
    defer urls.deinit();
    try std.testing.expectError(EnvVarsErrors.MissingDatabaseUrlApi, validateLoaded(urls));
}

// Auth-session storage lives in Redis; the API process must fail-fast at
// boot if REDIS_URL_API is missing rather than silently fall back to an
// in-memory store. Pins the no-in-memory-session-map invariant from the
// CLI device-flow spec.
test "validateLoaded rejects missing API Redis URL" {
    var urls = testEnvVars("postgres://api:pw@db.local:5432/api", null);
    defer urls.deinit();
    try std.testing.expectError(EnvVarsErrors.MissingRedisUrlApi, validateLoaded(urls));
}

test "validateLoaded rejects whitespace-only API Redis URL" {
    var urls = testEnvVars("postgres://api:pw@db.local:5432/api", "  \t\n");
    defer urls.deinit();
    try std.testing.expectError(EnvVarsErrors.MissingRedisUrlApi, validateLoaded(urls));
}

test "validateLoaded rejects non-TLS API Redis" {
    var urls = testEnvVars("postgres://api:pw@db.local:5432/api", "redis://api:pw@cache.local:6379");
    defer urls.deinit();
    try std.testing.expectError(EnvVarsErrors.RedisApiTlsRequired, validateLoaded(urls));
}

test "validateLoaded accepts valid API URLs" {
    var urls = testEnvVars("postgres://api:pw@db.local:5432/api", "rediss://api:pw@cache.local:6379");
    defer urls.deinit();
    try validateLoaded(urls);
}
