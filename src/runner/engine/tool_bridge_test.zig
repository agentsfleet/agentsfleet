//! Tests for `tool_bridge.zig`. A sibling file, not inline blocks: the product
//! file sat at 328 of the 350-line cap with no room for the arms below, and the
//! cap is a ceiling rather than a budget to spend.

const std = @import("std");
const nullclaw = @import("nullclaw");
const Config = nullclaw.config.Config;

const tool_bridge = @import("tool_bridge.zig");
const resolve = tool_bridge.resolve;
const buildTools = tool_bridge.buildTools;
const TOOL_COUNT = tool_bridge.TOOL_COUNT;
const UNSUPPORTED_HOSTED_TOOLS = tool_bridge.UNSUPPORTED_HOSTED_TOOLS;
const isUnsupportedHostedToolName = tool_bridge.isUnsupportedHostedToolName;

test "resolve: canonical name found" {
    const entry = resolve("file_read").?;
    try std.testing.expectEqualStrings("file_read", entry.name);
}

test "resolve: all core tools resolvable" {
    const core = [_][]const u8{
        "shell",         "file_read",    "file_write",       "file_edit",
        "file_append",   "file_delete",  "file_read_hashed", "file_edit_hashed",
        "git",           "image",        "calculator",       "memory_store",
        "memory_recall", "memory_list",  "memory_forget",    "delegate",
        "spawn",         "http_request", "web_search",       "web_fetch",
        "pushover",      "browser",      "screenshot",       "browser_open",
        "message",
    };
    for (core) |name| {
        try std.testing.expect(resolve(name) != null);
    }
    try std.testing.expectEqual(@as(usize, core.len), TOOL_COUNT);
}

test "resolve: hosted local scheduler tools are unsupported" {
    for (UNSUPPORTED_HOSTED_TOOLS) |name| {
        try std.testing.expect(resolve(name) == null);
        try std.testing.expect(isUnsupportedHostedToolName(name));
    }
}

test "resolve: unknown name returns null" {
    try std.testing.expect(resolve("linear") == null);
    try std.testing.expect(resolve("slack") == null);
    try std.testing.expect(resolve("") == null);
}

test "buildTools: empty array returns empty slice" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "buildTools: non-array value returns empty slice" {
    const alloc = std.testing.allocator;
    const result = try buildTools(alloc, .{ .integer = 42 }, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
}

test "buildTools: unknown tool name skipped and reported" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "name", .{ .string = "unknown_future_tool" });
    try arr.array.append(.{ .object = obj });
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 1), result.skipped.len);
    try std.testing.expectEqualStrings("unknown_future_tool", result.skipped[0]);
}

test "buildTools: disabled tool skipped" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "name", .{ .string = "file_read" });
    try obj.put(alloc, "enabled", .{ .bool = false });
    try arr.array.append(.{ .object = obj });
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "a tool that cannot be appended is freed rather than stranded mid-build" {
    // Each tool is constructed, then appended. A failure between the two leaves
    // a fully-built tool owned by nobody — and a tool can hold an allocator's
    // worth of state, so this is not a stray pointer but the whole tool. The
    // sweep walks the failure across the spec until it lands in that gap;
    // testing.allocator underneath fails the test on anything left behind.
    const spec_json =
        \\[{"name":"file_read","enabled":true},
        \\ {"name":"file_write","enabled":true},
        \\ {"name":"shell","enabled":true}]
    ;
    for (0..12) |fail_index| {
        var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, spec_json, .{});
        defer parsed.deinit();
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        const alloc = failing.allocator();
        var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = alloc };

        if (buildTools(alloc, parsed.value, "/tmp", &cfg, null, null)) |result| {
            result.deinit(alloc);
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
        }
    }
}
