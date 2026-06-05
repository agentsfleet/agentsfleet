//! TTL eviction pin for the Redis-backed SessionStore.
//!
//! The spec dimension calls for "create session; wait 5min 30s; assert
//! EXISTS returns 0" — a 330-second wall-clock test that would block
//! CI for over five minutes. The truth that matters is that the SET
//! EX argument matches SESSION_TTL_SECONDS; the actual eviction is
//! Redis's job. So this test PTTLs a freshly-created key and asserts
//! the remaining lifetime is within the spec'd 300-second window.
//!
//! Lives in its own file so the verify-path + pod-survival tests in
//! session_store_redis_integration_test.zig can stay under the 350-line
//! RULE FLL cap. Self-skips on TEST_REDIS_TLS_URL the same way.

const std = @import("std");
const common = @import("common");
const queue_redis = @import("../queue/redis.zig");
const session_store_redis = @import("session_store_redis.zig");

const TEST_REDIS_URL_ENV: [:0]const u8 = "TEST_REDIS_TLS_URL";
const TEST_CODE_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";
const TEST_AUDIT_PEPPER: []const u8 = "test-audit-pepper-bytes-32--pad_";
const TEST_CLI_PK: []const u8 = "BASE64URL_SPKI_CLI_PUBLIC_KEY_PLACEHOLDER";
const TEST_TOKEN_NAME: []const u8 = "ttl-test";

fn connectRedisOrSkip(alloc: std.mem.Allocator) !queue_redis.Client {
    const url = common.env.testLiveValue(TEST_REDIS_URL_ENV) orelse return error.SkipZigTest;
    return queue_redis.testing.connectFromUrl(common.globalIo(), alloc, url);
}

fn delKey(client: *queue_redis.Client, alloc: std.mem.Allocator, key: []const u8) void {
    var resp = client.command(&.{ "DEL", key }) catch return;
    resp.deinit(alloc);
}

test "session key inherits SESSION_TTL_SECONDS on create" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = session_store_redis.SessionStore.init(
        alloc,
        &client,
        TEST_CODE_PEPPER,
        TEST_AUDIT_PEPPER,
    );

    const sid = try store.create(TEST_CLI_PK, TEST_TOKEN_NAME);
    defer alloc.free(sid);

    var key_buf: [session_store_redis.SESSION_KEY_PREFIX.len + 36]u8 = undefined;
    const key = try std.fmt.bufPrint(
        &key_buf,
        "{s}{s}",
        .{ session_store_redis.SESSION_KEY_PREFIX, sid },
    );
    defer delKey(&client, alloc, key);

    var resp = try client.command(&.{ "PTTL", key });
    defer resp.deinit(alloc);

    const pttl_ms = switch (resp) {
        .integer => |i| i,
        else => return error.UnexpectedRedisReply,
    };

    // PTTL == -1 means no TTL set (would be a spec violation); -2 means
    // key doesn't exist. Both surface as `pttl_ms <= 0` against our
    // positive-range assertion.
    const max_ms: i64 = @as(i64, session_store_redis.SESSION_TTL_SECONDS) * 1000;

    try std.testing.expect(pttl_ms > 0);
    try std.testing.expect(pttl_ms <= max_ms);

    // 5-second safety margin covers the gap between create() and PTTL
    // call. If the TTL was set to anything materially shorter (say 30s
    // or 60s), this lower bound trips.
    try std.testing.expect(pttl_ms >= max_ms - 5_000);
}
