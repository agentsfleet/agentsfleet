//! Unwind tests for `hosted_tools.filterUnsupported`. A sibling file, not an
//! inline block: the fake tool below is test support, and test support living
//! in a product file is counted as product by the coverage denominator — where
//! its unused vtable stubs read as permanently dark shipped code.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const hosted_tools = @import("hosted_tools.zig");
const filterUnsupported = hosted_tools.filterUnsupported;

// ---------------------------------------------------------------------------
// The filter owns two heap sets at once — the incoming slice and the outgoing
// list — and swaps ownership between them mid-function. Both unwind arms are
// driven here, because a leak on this path leaks every tool a fleet was built
// with, and the success path alone never touches them.
// ---------------------------------------------------------------------------

/// A tool that owns nothing but a heap box and a deinit tally, so a test can
/// prove the filter freed exactly what it claimed to.
const FakeTool = struct {
    tool_name: []const u8,
    freed: *usize,

    const vtable = tools_mod.Tool.VTable{
        .execute = execute,
        .name = name,
        .description = description,
        .parameters_json = parametersJson,
        .deinit = deinitFn,
    };

    fn create(alloc: std.mem.Allocator, tool_name: []const u8, freed: *usize) !tools_mod.Tool {
        const self = try alloc.create(FakeTool);
        self.* = .{ .tool_name = tool_name, .freed = freed };
        return .{ .ptr = self, .vtable = &vtable };
    }

    fn execute(_: *anyopaque, _: std.mem.Allocator, _: nullclaw.tools.JsonObjectMap) anyerror!tools_mod.ToolResult {
        return tools_mod.ToolResult.ok("");
    }

    fn name(ptr: *anyopaque) []const u8 {
        return @as(*FakeTool, @ptrCast(@alignCast(ptr))).tool_name;
    }

    fn description(_: *anyopaque) []const u8 {
        return "fake";
    }

    fn parametersJson(_: *anyopaque) []const u8 {
        return "{}";
    }

    fn deinitFn(ptr: *anyopaque, alloc: std.mem.Allocator) void {
        const self: *FakeTool = @ptrCast(@alignCast(ptr));
        self.freed.* += 1;
        alloc.destroy(self);
    }
};

/// `schedule` is one of the local scheduler tools hosted runs must never see;
/// `file_read` stands for everything that survives the filter.
const UNSUPPORTED_NAME = "schedule";
const SUPPORTED_NAME = "file_read";

fn fakeTools(alloc: std.mem.Allocator, freed: *usize) ![]tools_mod.Tool {
    const tools = try alloc.alloc(tools_mod.Tool, 3);
    errdefer alloc.free(tools);
    tools[0] = try FakeTool.create(alloc, SUPPORTED_NAME, freed);
    tools[1] = try FakeTool.create(alloc, UNSUPPORTED_NAME, freed);
    tools[2] = try FakeTool.create(alloc, SUPPORTED_NAME, freed);
    return tools;
}

test "the filter drops local scheduler tools and frees them rather than leaking" {
    const alloc = std.testing.allocator;
    var freed: usize = 0;
    const kept = try filterUnsupported(alloc, try fakeTools(alloc, &freed));
    defer {
        for (kept) |t| t.deinit(alloc);
        alloc.free(kept);
    }

    try std.testing.expectEqual(@as(usize, 2), kept.len);
    // Dropped is not the same as forgotten: the one removed tool is freed as it
    // goes, so filtering cannot become the leak it exists to prevent.
    try std.testing.expectEqual(@as(usize, 1), freed);
    for (kept) |t| try std.testing.expectEqualStrings(SUPPORTED_NAME, t.name());
}

test "both unwind arms free every tool when an allocation fails mid-filter" {
    // Each index is one allocation the filter makes after the tools exist. The
    // early ones leave the incoming slice owned, the late ones leave the
    // outgoing list owned — the two arms are mutually exclusive, and a fail at
    // any index must still leave nothing behind. testing.allocator underneath
    // fails the test on a single byte that survives.
    for (0..4) |fail_index| {
        var freed: usize = 0;
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index + 4 });
        const alloc = failing.allocator();
        const tools = fakeTools(alloc, &freed) catch continue;
        if (filterUnsupported(alloc, tools)) |kept| {
            // The budget outlived the filter: nothing to assert, just clean up.
            for (kept) |t| t.deinit(alloc);
            alloc.free(kept);
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            try std.testing.expectEqual(@as(usize, 3), freed);
        }
    }
}
