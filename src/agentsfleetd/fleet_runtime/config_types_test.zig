const std = @import("std");
const config_types = @import("config_types.zig");

const FleetStatus = config_types.FleetStatus;

test "FleetStatus.toSlice round-trips via fromSlice" {
    inline for (&[_]FleetStatus{ .active, .paused, .stopped, .killed, .installing }) |s| {
        const text = s.toSlice();
        const parsed = FleetStatus.fromSlice(text) orelse return error.RoundTripFailed;
        try std.testing.expectEqual(s, parsed);
    }
}

test "FleetStatus.fromSlice rejects unknown labels" {
    try std.testing.expect(FleetStatus.fromSlice("") == null);
    try std.testing.expect(FleetStatus.fromSlice("running") == null);
    try std.testing.expect(FleetStatus.fromSlice("Active") == null); // case-sensitive
}

test "FleetStatus.isTerminal only true for killed" {
    try std.testing.expect(!FleetStatus.active.isTerminal());
    try std.testing.expect(!FleetStatus.paused.isTerminal());
    try std.testing.expect(!FleetStatus.stopped.isTerminal());
    try std.testing.expect(!FleetStatus.installing.isTerminal());
    try std.testing.expect(FleetStatus.killed.isTerminal());
}

test "FleetStatus.isRunnable only true for active" {
    try std.testing.expect(FleetStatus.active.isRunnable());
    try std.testing.expect(!FleetStatus.paused.isRunnable());
    try std.testing.expect(!FleetStatus.stopped.isRunnable());
    try std.testing.expect(!FleetStatus.killed.isRunnable());
    try std.testing.expect(!FleetStatus.installing.isRunnable());
}

test "validRequiredTags accepts bounded sets and rejects invalid lengths" {
    try std.testing.expect(config_types.validRequiredTags(&.{}));
    try std.testing.expect(config_types.validRequiredTags(&.{ "gpu", "us-east" }));
    try std.testing.expect(!config_types.validRequiredTags(&.{""}));
    try std.testing.expect(!config_types.validRequiredTags(&.{ "gpu", "" }));

    const tag_at_max = "a" ** 64;
    const tag_over_max = "a" ** 65;
    try std.testing.expect(config_types.validRequiredTags(&.{tag_at_max}));
    try std.testing.expect(!config_types.validRequiredTags(&.{tag_over_max}));

    var at_max: [32][]const u8 = undefined;
    for (&at_max) |*tag| tag.* = "x";
    try std.testing.expect(config_types.validRequiredTags(&at_max));
    var over_max: [33][]const u8 = undefined;
    for (&over_max) |*tag| tag.* = "x";
    try std.testing.expect(!config_types.validRequiredTags(&over_max));
}
