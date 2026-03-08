const std = @import("std");
const topology = @import("topology.zig");
const worker_runtime = @import("worker_runtime.zig");

pub const StageTransition = union(enum) {
    stage_index: usize,
    done,
    retry,
    blocked,
};

pub fn resolveStageTransition(profile: *const topology.Profile, current_index: usize, passed: bool) !StageTransition {
    const stage = profile.stages[current_index];
    const explicit_target = if (passed) stage.on_pass else stage.on_fail;

    if (explicit_target) |target| {
        if (std.ascii.eqlIgnoreCase(target, topology.TRANSITION_DONE)) return .done;
        if (std.ascii.eqlIgnoreCase(target, topology.TRANSITION_RETRY)) return .retry;
        if (std.ascii.eqlIgnoreCase(target, topology.TRANSITION_BLOCKED)) return .blocked;
        if (profile.indexOfStage(target)) |index| return .{ .stage_index = index };
        return worker_runtime.WorkerError.InvalidPipelineProfile;
    }

    if (passed) {
        if (current_index + 1 < profile.stages.len) return .{ .stage_index = current_index + 1 };
        return .done;
    }
    return .retry;
}

test "integration: stage transition graph executes pass branch to done" {
    var profile = try topology.defaultProfile(std.testing.allocator);
    defer profile.deinit();

    const verify_idx = profile.indexOfStage(topology.STAGE_VERIFY) orelse return error.TestExpectedEqual;
    const transition = try resolveStageTransition(&profile, verify_idx, true);
    try std.testing.expectEqual(StageTransition.done, transition);
}

test "integration: stage transition graph executes fail branch to retry" {
    var profile = try topology.defaultProfile(std.testing.allocator);
    defer profile.deinit();

    const verify_idx = profile.indexOfStage(topology.STAGE_VERIFY) orelse return error.TestExpectedEqual;
    const transition = try resolveStageTransition(&profile, verify_idx, false);
    try std.testing.expectEqual(StageTransition.retry, transition);
}

test "integration: invalid explicit target fails closed" {
    var profile = try topology.defaultProfile(std.testing.allocator);
    defer profile.deinit();

    const verify_idx = profile.indexOfStage(topology.STAGE_VERIFY) orelse return error.TestExpectedEqual;
    profile.stages[verify_idx].on_fail = "missing-stage";

    try std.testing.expectError(worker_runtime.WorkerError.InvalidPipelineProfile, resolveStageTransition(&profile, verify_idx, false));
}
