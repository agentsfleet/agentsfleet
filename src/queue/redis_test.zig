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

test "integration: rediss ping via REDIS_TLS_TEST_URL" {
    const alloc = std.testing.allocator;
    const tls_url = std.process.getEnvVarOwned(alloc, "REDIS_TLS_TEST_URL") catch return error.SkipZigTest;
    defer alloc.free(tls_url);

    if (!std.mem.startsWith(u8, tls_url, "rediss://")) return error.SkipZigTest;

    var client = try redis.testing.connectFromUrl(alloc, tls_url);
    defer client.deinit();
    try client.ping();
}

test "integration: ensureConsumerGroup is idempotent for readiness checks" {
    const alloc = std.testing.allocator;
    const redis_url = std.process.getEnvVarOwned(alloc, "REDIS_READY_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "REDIS_TLS_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "REDIS_URL") catch return error.SkipZigTest;
    defer alloc.free(redis_url);

    var client = try redis.testing.connectFromUrl(alloc, redis_url);
    defer client.deinit();

    try client.ensureConsumerGroup();
    try client.ensureConsumerGroup();
}

test "roleEnvVarName maps redis roles deterministically" {
    try std.testing.expectEqualStrings("REDIS_URL", redis.roleEnvVarName(.default));
    try std.testing.expectEqualStrings("REDIS_URL_API", redis.roleEnvVarName(.api));
    try std.testing.expectEqualStrings("REDIS_URL_WORKER", redis.roleEnvVarName(.worker));
}
