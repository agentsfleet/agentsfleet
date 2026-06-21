const std = @import("std");
const config_types = @import("config_types.zig");

const FleetStatus = config_types.FleetStatus;

test "FleetStatus.toSlice round-trips via fromSlice" {
    inline for (&[_]FleetStatus{ .active, .paused, .stopped, .killed }) |s| {
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
    try std.testing.expect(FleetStatus.killed.isTerminal());
}

test "FleetStatus.isRunnable only true for active" {
    try std.testing.expect(FleetStatus.active.isRunnable());
    try std.testing.expect(!FleetStatus.paused.isRunnable());
    try std.testing.expect(!FleetStatus.stopped.isRunnable());
    try std.testing.expect(!FleetStatus.killed.isRunnable());
}
