const std = @import("std");
const env = @import("runtime_env_parse.zig");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

test "parseI16Env parses signed short values" {
    const alloc = std.testing.allocator;
    try std.posix.setenv("API_HTTP_THREADS", "3", true);
    defer std.posix.unsetenv("API_HTTP_THREADS");

    const value = try env.parseI16Env(alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
    try std.testing.expectEqual(@as(i16, 3), value);
}

test "parseI16Env returns default when env var unset" {
    const alloc = std.testing.allocator;
    std.posix.unsetenv("DEFINITELY_UNSET_HTTP_THREADS_PARSE_TEST");
    const value = try env.parseI16Env(alloc, "DEFINITELY_UNSET_HTTP_THREADS_PARSE_TEST", 7, ValidationError.InvalidApiHttpThreads);
    try std.testing.expectEqual(@as(i16, 7), value);
}

test "parseI16Env returns invalid_error on garbage" {
    const alloc = std.testing.allocator;
    try std.posix.setenv("BAD_HTTP_THREADS_PARSE_TEST", "not-a-number", true);
    defer std.posix.unsetenv("BAD_HTTP_THREADS_PARSE_TEST");
    try std.testing.expectError(
        ValidationError.InvalidApiHttpThreads,
        env.parseI16Env(alloc, "BAD_HTTP_THREADS_PARSE_TEST", 1, ValidationError.InvalidApiHttpThreads),
    );
}

test "parseOptionalI64Env returns null when env var unset" {
    const alloc = std.testing.allocator;
    std.posix.unsetenv("DEFINITELY_UNSET_QUEUE_DEPTH_PARSE_TEST");
    const value = try env.parseOptionalI64Env(alloc, "DEFINITELY_UNSET_QUEUE_DEPTH_PARSE_TEST", ValidationError.InvalidReadyMaxQueueDepth);
    try std.testing.expect(value == null);
}

test "envOrDefaultOwned dupes default when env var unset" {
    const alloc = std.testing.allocator;
    std.posix.unsetenv("DEFINITELY_UNSET_CACHE_ROOT_PARSE_TEST");
    const value = try env.envOrDefaultOwned(alloc, "DEFINITELY_UNSET_CACHE_ROOT_PARSE_TEST", "/default/path");
    defer alloc.free(value);
    try std.testing.expectEqualStrings("/default/path", value);
}
