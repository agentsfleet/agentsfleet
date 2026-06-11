//! Per-execution policy parsing. The policy *types* live in the dependency-free
//! `execution_policy` module (shared with the `/v1/runners` protocol so the
//! frozen wire surface needn't import runner parsing internals); this file
//! re-exports them so existing `context_budget.X` call sites stay stable.

const std = @import("std");
const execution_policy = @import("contract").execution_policy;

// Re-export the policy types — existing runner call sites use `context_budget.X`.
pub const ContextBudget = execution_policy.ContextBudget;
pub const ExecutionPolicy = execution_policy.ExecutionPolicy;

test "ContextBudget.applyDefaults substitutes the three auto-sentinel knobs" {
    var cb: ContextBudget = .{ .tool_window = 0, .memory_checkpoint_every = 0, .stage_chunk_threshold = 0.0 };
    cb.applyDefaults();
    try std.testing.expectEqual(execution_policy.DEFAULT_TOOL_WINDOW, cb.tool_window);
    try std.testing.expectEqual(execution_policy.DEFAULT_MEMORY_CHECKPOINT_EVERY, cb.memory_checkpoint_every);
    try std.testing.expectEqual(execution_policy.DEFAULT_STAGE_CHUNK_THRESHOLD, cb.stage_chunk_threshold);
}

test "ContextBudget.applyDefaults preserves operator overrides" {
    var cb: ContextBudget = .{ .tool_window = 8, .memory_checkpoint_every = 3, .stage_chunk_threshold = 0.6 };
    cb.applyDefaults();
    try std.testing.expectEqual(@as(u32, 8), cb.tool_window);
    try std.testing.expectEqual(@as(u32, 3), cb.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.6), cb.stage_chunk_threshold);
}

test "ContextBudget.applyDefaults leaves model and context_cap_tokens untouched" {
    var cb: ContextBudget = .{ .model = "claude-sonnet-4-6", .context_cap_tokens = 0 };
    cb.applyDefaults();
    try std.testing.expectEqualStrings("claude-sonnet-4-6", cb.model);
    try std.testing.expectEqual(@as(u32, 0), cb.context_cap_tokens);
}
