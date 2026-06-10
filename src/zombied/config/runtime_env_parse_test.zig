const std = @import("std");
const common = @import("common");
const env = @import("runtime_env_parse.zig");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

fn envOf(pairs: []const [2][]const u8) !common.env.Map {
    return common.env.fromPairs(std.testing.allocator, pairs);
}

test "parseI16Env parses signed short values" {
    const alloc = std.testing.allocator;
    var env_map = try envOf(&.{.{ "API_HTTP_THREADS", "3" }});
    defer env_map.deinit();

    const value = try env.parseI16Env(&env_map, alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
    try std.testing.expectEqual(@as(i16, 3), value);
}

test "parseI16Env returns default when env var unset" {
    const alloc = std.testing.allocator;
    var env_map = try envOf(&.{});
    defer env_map.deinit();
    const value = try env.parseI16Env(&env_map, alloc, "DEFINITELY_UNSET_HTTP_THREADS_PARSE_TEST", 7, ValidationError.InvalidApiHttpThreads);
    try std.testing.expectEqual(@as(i16, 7), value);
}

test "parseI16Env returns invalid_error on garbage" {
    const alloc = std.testing.allocator;
    var env_map = try envOf(&.{.{ "BAD_HTTP_THREADS_PARSE_TEST", "not-a-number" }});
    defer env_map.deinit();
    try std.testing.expectError(
        ValidationError.InvalidApiHttpThreads,
        env.parseI16Env(&env_map, alloc, "BAD_HTTP_THREADS_PARSE_TEST", 1, ValidationError.InvalidApiHttpThreads),
    );
}

test "parseOptionalI64Env returns null when env var unset" {
    const alloc = std.testing.allocator;
    var env_map = try envOf(&.{});
    defer env_map.deinit();
    const value = try env.parseOptionalI64Env(&env_map, alloc, "DEFINITELY_UNSET_QUEUE_DEPTH_PARSE_TEST", ValidationError.InvalidReadyMaxQueueDepth);
    try std.testing.expect(value == null);
}

test "envOrDefaultOwned dupes default when env var unset" {
    const alloc = std.testing.allocator;
    var env_map = try envOf(&.{});
    defer env_map.deinit();
    const value = try env.envOrDefaultOwned(&env_map, alloc, "DEFINITELY_UNSET_CACHE_ROOT_PARSE_TEST", "/default/path");
    defer alloc.free(value);
    try std.testing.expectEqualStrings("/default/path", value);
}
