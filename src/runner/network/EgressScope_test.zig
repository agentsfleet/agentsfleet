//! EgressScope_test.zig — pure-half unit tests for EgressScope (FLL-exempt
//! sibling). The kernel behaviour (create/attach/destroy on real netns + nft)
//! is the Linux integration lane in `egress_integration_test.zig`.

const std = @import("std");
const builtin = @import("builtin");
const Plan = @import("Plan.zig");
const EgressScope = @import("EgressScope.zig");

test "create is fail-closed off Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    var p = try Plan.build(std.testing.allocator, 0, &.{});
    defer p.deinit();
    try std.testing.expectError(error.UnsupportedPlatform, EgressScope.create(std.testing.allocator, &p));
}

test "setName + tableName derive distinct per-worker names" {
    var buf: [16]u8 = undefined;
    try std.testing.expectEqualStrings("allow0", EgressScope.setName(&buf, 0));
    try std.testing.expectEqualStrings("allow253", EgressScope.setName(&buf, 253));
    var tbuf: [16]u8 = undefined;
    // Per-worker table names — the fix for the cross-worker teardown wipe.
    try std.testing.expectEqualStrings("uz_egress0", EgressScope.tableName(&tbuf, 0));
    try std.testing.expectEqualStrings("uz_egress7", EgressScope.tableName(&tbuf, 7));
}

test "collectV4 gathers v4 bytes and fails closed on a v6 entry" {
    const al = std.testing.allocator;
    const v4 = [_]Plan.HostEntry{
        .{ .name = "a", .addr = .{ .ip4 = .{ .bytes = .{ 1, 2, 3, 4 }, .port = 0 } } },
    };
    const got = try EgressScope.collectV4(al, &v4);
    defer al.free(got);
    try std.testing.expectEqual([4]u8{ 1, 2, 3, 4 }, got[0]);

    const v6 = [_]Plan.HostEntry{
        .{ .name = "b", .addr = try std.Io.net.IpAddress.parseIp6("::1", 0) },
    };
    try std.testing.expectError(error.UnsupportedAddressFamily, EgressScope.collectV4(al, &v6));
}
