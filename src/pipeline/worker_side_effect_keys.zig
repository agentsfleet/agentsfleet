const std = @import("std");

pub const SideEffectKey = struct {
    value: []const u8,
    owned: ?[]u8 = null,

    pub fn deinit(self: SideEffectKey, alloc: std.mem.Allocator) void {
        if (self.owned) |k| alloc.free(k);
    }
};

pub fn sideEffectKeyPush(alloc: std.mem.Allocator, buf: []u8, branch: []const u8) !SideEffectKey {
    const value = std.fmt.bufPrint(buf, "git:push:{s}", .{branch}) catch |err| blk: {
        if (err != error.NoSpaceLeft) return err;
        const owned = try std.fmt.allocPrint(alloc, "git:push:{s}", .{branch});
        break :blk owned;
    };
    const owned = if (@intFromPtr(value.ptr) == @intFromPtr(buf.ptr)) null else value;
    return .{ .value = value, .owned = owned };
}

pub fn sideEffectKeyPrCreate(alloc: std.mem.Allocator, buf: []u8, branch: []const u8) !SideEffectKey {
    const value = std.fmt.bufPrint(buf, "pr:create:{s}", .{branch}) catch |err| blk: {
        if (err != error.NoSpaceLeft) return err;
        const owned = try std.fmt.allocPrint(alloc, "pr:create:{s}", .{branch});
        break :blk owned;
    };
    const owned = if (@intFromPtr(value.ptr) == @intFromPtr(buf.ptr)) null else value;
    return .{ .value = value, .owned = owned };
}

test "integration: side effect key format for pr create is stable" {
    var buf: [128]u8 = undefined;
    const key = try sideEffectKeyPrCreate(std.testing.allocator, &buf, "zombie/run-r_abc");
    defer key.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("pr:create:zombie/run-r_abc", key.value);
}

test "integration: side effect key format for push is stable" {
    var buf: [128]u8 = undefined;
    const key = try sideEffectKeyPush(std.testing.allocator, &buf, "zombie/run-r_abc");
    defer key.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("git:push:zombie/run-r_abc", key.value);
}

test "integration: side effect key helpers fall back to heap for long branches" {
    const long_branch = "zombie/run-abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    var tiny: [16]u8 = undefined;

    const push_key = try sideEffectKeyPush(std.testing.allocator, &tiny, long_branch);
    defer push_key.deinit(std.testing.allocator);
    try std.testing.expect(std.mem.eql(u8, push_key.value, "git:push:zombie/run-abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"));
    try std.testing.expect(push_key.owned != null);

    const pr_key = try sideEffectKeyPrCreate(std.testing.allocator, &tiny, long_branch);
    defer pr_key.deinit(std.testing.allocator);
    try std.testing.expect(std.mem.eql(u8, pr_key.value, "pr:create:zombie/run-abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"));
    try std.testing.expect(pr_key.owned != null);
}
