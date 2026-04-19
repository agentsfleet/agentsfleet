const std = @import("std");
const config_types = @import("config_types.zig");

const ZombieStatus = config_types.ZombieStatus;

test "ZombieStatus.toSlice round-trips via fromSlice" {
    inline for (&[_]ZombieStatus{ .active, .paused, .stopped, .killed }) |s| {
        const text = s.toSlice();
        const parsed = ZombieStatus.fromSlice(text) orelse return error.RoundTripFailed;
        try std.testing.expectEqual(s, parsed);
    }
}

test "ZombieStatus.fromSlice rejects unknown labels" {
    try std.testing.expect(ZombieStatus.fromSlice("") == null);
    try std.testing.expect(ZombieStatus.fromSlice("running") == null);
    try std.testing.expect(ZombieStatus.fromSlice("Active") == null); // case-sensitive
}

test "ZombieStatus.isTerminal only true for killed" {
    try std.testing.expect(!ZombieStatus.active.isTerminal());
    try std.testing.expect(!ZombieStatus.paused.isTerminal());
    try std.testing.expect(!ZombieStatus.stopped.isTerminal());
    try std.testing.expect(ZombieStatus.killed.isTerminal());
}

test "ZombieStatus.isRunnable only true for active" {
    try std.testing.expect(ZombieStatus.active.isRunnable());
    try std.testing.expect(!ZombieStatus.paused.isRunnable());
    try std.testing.expect(!ZombieStatus.stopped.isRunnable());
    try std.testing.expect(!ZombieStatus.killed.isRunnable());
}
