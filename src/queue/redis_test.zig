const std = @import("std");
const redis = @import("redis.zig");

test "parseRedisUrl handles redis URL variants" {
    const alloc = std.testing.allocator;

    const local = try redis.testing.parseRedisUrl(alloc, "redis://localhost:6379");
    defer redis.testing.deinitConfig(alloc, local);
    try std.testing.expectEqualStrings("localhost", local.host);
    try std.testing.expectEqual(@as(u16, 6379), local.port);
    try std.testing.expect(!local.use_tls);

    const auth = try redis.testing.parseRedisUrl(alloc, "redis://user:pass@cache.local:6380");
    defer redis.testing.deinitConfig(alloc, auth);
    try std.testing.expectEqualStrings("cache.local", auth.host);
    try std.testing.expectEqualStrings("user", auth.username.?);
    try std.testing.expectEqualStrings("pass", auth.password.?);

    const tls = try redis.testing.parseRedisUrl(alloc, "rediss://default:secret@upstash.local:6379");
    defer redis.testing.deinitConfig(alloc, tls);
    try std.testing.expect(tls.use_tls);
}

test "parseRedisUrl supports rediss default port" {
    const alloc = std.testing.allocator;
    const tls = try redis.testing.parseRedisUrl(alloc, "rediss://localhost");
    defer redis.testing.deinitConfig(alloc, tls);
    try std.testing.expectEqualStrings("localhost", tls.host);
    try std.testing.expectEqual(@as(u16, 6379), tls.port);
    try std.testing.expect(tls.use_tls);
}

test "parseRedisUrl rejects invalid scheme and invalid port" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "http://localhost:6379"));
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "redis://localhost:notaport"));
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "redis://"));
}

test "loadCaBundle rejects relative custom CA path" {
    try std.testing.expectError(error.RedisTlsCaFileMustBeAbsolute, redis.testing.loadCaBundle(std.testing.allocator, "docker/redis/tls/ca.crt"));
}

test "integration: rediss ping via TEST_REDIS_TLS_URL" {
    const alloc = std.testing.allocator;
    const tls_url = std.process.getEnvVarOwned(alloc, "TEST_REDIS_TLS_URL") catch return error.SkipZigTest;
    defer alloc.free(tls_url);

    if (!std.mem.startsWith(u8, tls_url, "rediss://")) return error.SkipZigTest;

    var client = try redis.testing.connectFromUrl(alloc, tls_url);
    defer client.deinit();
    try client.ping();
}

test "roleEnvVarName maps redis roles deterministically" {
    try std.testing.expectEqualStrings("REDIS_URL", redis.roleEnvVarName(.default));
    try std.testing.expectEqualStrings("REDIS_URL_API", redis.roleEnvVarName(.api));
    try std.testing.expectEqualStrings("REDIS_URL_WORKER", redis.roleEnvVarName(.worker));
}

// ── Queue constants regression tests ─────────────────────────────────────

const queue_consts = @import("constants.zig");

test "queue constants: consumer prefix is stable" {
    try std.testing.expectEqualStrings("worker", queue_consts.consumer_prefix);
}


test "queue constants: XAUTOCLAIM cursor seed and batch size" {
    try std.testing.expectEqualStrings("0-0", queue_consts.xautoclaim_start);
    try std.testing.expectEqualStrings("1", queue_consts.xautoclaim_count);
}

// ── makeConsumerId tests ─────────────────────────────────────────────────

test "makeConsumerId starts with consumer prefix" {
    const alloc = std.testing.allocator;
    const id = try redis.makeConsumerId(alloc);
    defer alloc.free(id);
    try std.testing.expect(std.mem.startsWith(u8, id, queue_consts.consumer_prefix ++ "-"));
}

test "makeConsumerId produces unique IDs across calls" {
    const alloc = std.testing.allocator;
    const id1 = try redis.makeConsumerId(alloc);
    defer alloc.free(id1);
    const id2 = try redis.makeConsumerId(alloc);
    defer alloc.free(id2);
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

