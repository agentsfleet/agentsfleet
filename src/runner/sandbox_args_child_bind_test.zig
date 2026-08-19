//! The CHILD half of the bind set — what `sandbox_args` TELLS the forked child,
//! which is what the child's own landlock ruleset then admits.
//!
//! Split from sandbox_args_bind_test.zig on the 350-line bound (RULE FLL), which
//! was itself split from sandbox_args_edge_test.zig for the same reason. That
//! sibling owns the bwrap half: which host paths get MOUNTED, and at what mode.
//!
//! The two halves disagreeing is the whole M136 incident in one line — bwrap
//! mounted the path, landlock denied the read, and the lease died at first use
//! rather than at setup. So both halves are pinned, and both are platform
//! INDEPENDENT: `buildArgv` only reaches this emission on Linux, so proving it
//! through that door would prove it on one host and nowhere else.

const std = @import("std");
const contract = @import("contract");

const sandbox_args = @import("sandbox_args.zig");
const sandbox_hardening = @import("sandbox_hardening.zig");

fn indexOfStr(argv: []const []const u8, needle: []const u8) ?usize {
    for (argv, 0..) |s, i| {
        if (std.mem.eql(u8, s, needle)) return i;
    }
    return null;
}

/// Emit the child's bind flags for `extra`, caller frees.
fn childFlagsFor(alloc: std.mem.Allocator, extra: []const contract.protocol.ExtraBind) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |it| alloc.free(it);
        list.deinit(alloc);
    }
    try sandbox_args.appendBindFlags(alloc, &list, extra);
    return list.toOwnedSlice(alloc);
}

fn freeFlags(alloc: std.mem.Allocator, flags: []const []const u8) void {
    for (flags) |f| alloc.free(f);
    alloc.free(flags);
}

test "each operator bind reaches the child under the flag its own mode names" {
    const alloc = std.testing.allocator;
    const flags = try childFlagsFor(alloc, &.{
        .{ .path = "/srv/fonts" },
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
    });
    defer freeFlags(alloc, flags);

    // Built from the parser's own prefixes, never a re-spelled literal: the
    // child reads these back with `collectBindFlags`, so a test carrying its own
    // copy would stay green while the two sides drifted apart (RULE UFS).
    const want_ro = try std.fmt.allocPrint(alloc, "{s}{s}", .{ sandbox_hardening.BIND_RO_FLAG_PREFIX, "/srv/fonts" });
    defer alloc.free(want_ro);
    const want_rw = try std.fmt.allocPrint(alloc, "{s}{s}", .{ sandbox_hardening.BIND_RW_FLAG_PREFIX, "/srv/models" });
    defer alloc.free(want_rw);

    try std.testing.expect(indexOfStr(flags, want_ro) != null);
    try std.testing.expect(indexOfStr(flags, want_rw) != null);
    try std.testing.expectEqual(@as(usize, 2), flags.len);
}

test "a read-only bind is never emitted writable to the child" {
    const alloc = std.testing.allocator;
    // The mode arms carry different landlock access masks, so an arm that fell
    // through would hand a lease write access to a path the operator marked
    // read-only — silently, and only inside the sandbox.
    const flags = try childFlagsFor(alloc, &.{.{ .path = "/srv/fonts" }});
    defer freeFlags(alloc, flags);

    const wrong = try std.fmt.allocPrint(alloc, "{s}{s}", .{ sandbox_hardening.BIND_RW_FLAG_PREFIX, "/srv/fonts" });
    defer alloc.free(wrong);
    try std.testing.expect(indexOfStr(flags, wrong) == null);
}

test "no operator binds emits no child flags at all" {
    const alloc = std.testing.allocator;
    const flags = try childFlagsFor(alloc, &.{});
    defer freeFlags(alloc, flags);
    try std.testing.expectEqual(@as(usize, 0), flags.len);
}
