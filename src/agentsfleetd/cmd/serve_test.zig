//! Serve command arg-parsing tests — extracted from serve.zig
//! for the 350-line limit (M10_002).

const std = @import("std");
const serve_args = @import("serve_args.zig");
const ServeArgError = serve_args.ServeArgError;

const TestArgIterator = struct {
    args: []const []const u8,
    index: usize = 0,

    pub fn next(self: *TestArgIterator) ?[]const u8 {
        if (self.index >= self.args.len) return null;
        const arg = self.args[self.index];
        self.index += 1;
        return arg;
    }
};

// --- T1: Happy path — parseServeArgs with valid --port ---

test "parseServeArgs returns null when no args" {
    var it = TestArgIterator{ .args = &.{} };
    const result = try serve_args.parseArgs(&it);
    try std.testing.expect(result == null);
}

test "parseServeArgs parses --port with space" {
    var it = TestArgIterator{ .args = &.{ "--port", "8080" } };
    const result = try serve_args.parseArgs(&it);
    try std.testing.expectEqual(@as(?u16, 8080), result);
}

test "parseServeArgs parses --port= form" {
    var it = TestArgIterator{ .args = &.{"--port=3001"} };
    const result = try serve_args.parseArgs(&it);
    try std.testing.expectEqual(@as(?u16, 3001), result);
}

// --- T2: Edge cases ---

test "parseServeArgs rejects port 0" {
    var it = TestArgIterator{ .args = &.{ "--port", "0" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, serve_args.parseArgs(&it));
}

test "parseServeArgs rejects port=0 form" {
    var it = TestArgIterator{ .args = &.{"--port=0"} };
    try std.testing.expectError(ServeArgError.InvalidPortValue, serve_args.parseArgs(&it));
}

test "parseServeArgs rejects non-numeric port" {
    var it = TestArgIterator{ .args = &.{ "--port", "abc" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, serve_args.parseArgs(&it));
}

test "parseServeArgs rejects overflow port" {
    var it = TestArgIterator{ .args = &.{ "--port", "99999" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, serve_args.parseArgs(&it));
}

test "parseServeArgs rejects negative port" {
    var it = TestArgIterator{ .args = &.{ "--port", "-1" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, serve_args.parseArgs(&it));
}

// --- T3: Error paths ---

test "parseServeArgs returns error for missing port value" {
    var it = TestArgIterator{ .args = &.{"--port"} };
    try std.testing.expectError(ServeArgError.MissingPortValue, serve_args.parseArgs(&it));
}

test "parseServeArgs returns error for unknown arg" {
    var it = TestArgIterator{ .args = &.{"--verbose"} };
    try std.testing.expectError(ServeArgError.InvalidServeArgument, serve_args.parseArgs(&it));
}

test "parseServeArgs returns error for unknown arg after valid port" {
    var it = TestArgIterator{ .args = &.{ "--port", "3000", "--extra" } };
    try std.testing.expectError(ServeArgError.InvalidServeArgument, serve_args.parseArgs(&it));
}

test "parsePortValue parses valid ports" {
    try std.testing.expectEqual(@as(?u16, 3000), serve_args.parsePortValue("3000"));
    try std.testing.expectEqual(@as(?u16, 1), serve_args.parsePortValue("1"));
    try std.testing.expectEqual(@as(?u16, 65535), serve_args.parsePortValue("65535"));
}

test "parsePortValue rejects invalid values" {
    try std.testing.expect(serve_args.parsePortValue("0") == null);
    try std.testing.expect(serve_args.parsePortValue("abc") == null);
    try std.testing.expect(serve_args.parsePortValue("99999") == null);
    try std.testing.expect(serve_args.parsePortValue("") == null);
    try std.testing.expect(serve_args.parsePortValue("-5") == null);
}
