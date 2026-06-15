const std = @import("std");
const config_types = @import("config_types.zig");

const AgentStatus = config_types.AgentStatus;

test "AgentStatus.toSlice round-trips via fromSlice" {
    inline for (&[_]AgentStatus{ .active, .paused, .stopped, .killed }) |s| {
        const text = s.toSlice();
        const parsed = AgentStatus.fromSlice(text) orelse return error.RoundTripFailed;
        try std.testing.expectEqual(s, parsed);
    }
}

test "AgentStatus.fromSlice rejects unknown labels" {
    try std.testing.expect(AgentStatus.fromSlice("") == null);
    try std.testing.expect(AgentStatus.fromSlice("running") == null);
    try std.testing.expect(AgentStatus.fromSlice("Active") == null); // case-sensitive
}

test "AgentStatus.isTerminal only true for killed" {
    try std.testing.expect(!AgentStatus.active.isTerminal());
    try std.testing.expect(!AgentStatus.paused.isTerminal());
    try std.testing.expect(!AgentStatus.stopped.isTerminal());
    try std.testing.expect(AgentStatus.killed.isTerminal());
}

test "AgentStatus.isRunnable only true for active" {
    try std.testing.expect(AgentStatus.active.isRunnable());
    try std.testing.expect(!AgentStatus.paused.isRunnable());
    try std.testing.expect(!AgentStatus.stopped.isRunnable());
    try std.testing.expect(!AgentStatus.killed.isRunnable());
}
