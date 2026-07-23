//! The dispatch contract: a target carries its own expected generation, so the
//! scheduler can fire it without knowing anything about the owner.

const std = @import("std");
const InterruptTarget = @import("InterruptTarget.zig");

const LIVE_GENERATION: u64 = 7;

const RecordingOwner = struct {
    seen_generation: u64 = 0,
    calls: usize = 0,

    fn interrupt(ctx: *anyopaque, expected_generation: u64) InterruptTarget.Outcome {
        const self: *RecordingOwner = @ptrCast(@alignCast(ctx));
        self.seen_generation = expected_generation;
        self.calls += 1;
        return if (expected_generation == LIVE_GENERATION) .interrupted else .stale;
    }

    fn target(self: *RecordingOwner, generation: u64) InterruptTarget {
        return .{ .ctx = self, .interruptFn = interrupt, .generation = generation };
    }
};

test "a target hands the owner the generation it was armed against" {
    var owner: RecordingOwner = .{};

    try std.testing.expectEqual(
        InterruptTarget.Outcome.interrupted,
        owner.target(LIVE_GENERATION).interrupt(),
    );
    try std.testing.expectEqual(LIVE_GENERATION, owner.seen_generation);
    try std.testing.expectEqual(@as(usize, 1), owner.calls);

    // The owner — not the scheduler — decides what is stale. The scheduler
    // holds no descriptor and makes no comparison of its own.
    try std.testing.expectEqual(
        InterruptTarget.Outcome.stale,
        owner.target(LIVE_GENERATION + 1).interrupt(),
    );
    try std.testing.expectEqual(LIVE_GENERATION + 1, owner.seen_generation);
    try std.testing.expectEqual(@as(usize, 2), owner.calls);
}

test "a target exposes no descriptor to the scheduler" {
    // The scheduler can only ever call `interrupt`. If a descriptor field were
    // ever added here, this fails and the review question gets asked.
    const fields = @typeInfo(InterruptTarget).@"struct".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
    inline for (fields) |field| {
        const named_handle = std.mem.eql(u8, field.name, "handle") or
            std.mem.eql(u8, field.name, "fd") or
            std.mem.eql(u8, field.name, "socket");
        try std.testing.expect(!named_handle);
    }
}
