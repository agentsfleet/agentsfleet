//! Unit tests for the engine's usage-split mapping — the seam where the
//! vendored agent's cumulative accessors become `ExecutionResult` splits.
//! Pins the verbatim mapping (prompt → input, completion → output) and the
//! cached-input-stays-zero limitation, so a pin bump that changes accessor
//! semantics fails here instead of in billing.

const std = @import("std");
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const providers = nullclaw.providers;
const Agent = nullclaw.agent.Agent;

const runner = @import("runner.zig");
const contract = @import("contract");

/// Field-literal Agent mirroring the upstream agent module's own tokens test:
/// `.provider = undefined` is never invoked (no run call in these tests), and
/// `deinit` owns the zero-length tool_specs allocation.
fn testAgent(allocator: std.mem.Allocator, noop: *observability.NoopObserver) !Agent {
    return .{
        .allocator = allocator,
        // SAFETY: never invoked — these tests read accessors only, no run call.
        .provider = undefined,
        .tools = &.{},
        .tool_specs = try allocator.alloc(providers.ToolSpec, 0),
        .mem = null,
        .observer = noop.observer(),
        .model_name = "test",
        .temperature = 0.7,
        .workspace_dir = "/tmp",
        .max_tool_iterations = 10,
        .max_history_messages = 50,
        .auto_save = false,
        .history = .empty,
        .total_tokens = 0,
        .has_system_prompt = false,
    };
}

test "usageSplits maps the agent's cumulative accessors verbatim" {
    var noop = observability.NoopObserver{};
    var agent = try testAgent(std.testing.allocator, &noop);
    defer agent.deinit();

    // A fresh agent maps to all-zero splits.
    const zero = runner.usageSplits(&agent);
    try std.testing.expectEqual(@as(u64, 0), zero.input);
    try std.testing.expectEqual(@as(u64, 0), zero.output);

    agent.prompt_tokens_total = 10;
    agent.completion_tokens_total = 5;
    agent.total_tokens = 15;

    const splits = runner.usageSplits(&agent);
    try std.testing.expectEqual(@as(u64, 10), splits.input);
    try std.testing.expectEqual(@as(u64, 5), splits.output);
    // The legacy total rides beside the splits, unchanged.
    try std.testing.expectEqual(@as(u64, 15), agent.tokensUsed());
}

test "ExecutionResult carries splits verbatim with cached pinned to zero" {
    const result = contract.execution_result.ExecutionResult{
        .input_tokens = 10,
        .output_tokens = 5,
        .token_count = 15,
    };
    try std.testing.expectEqual(@as(u64, 10), result.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), result.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 5), result.output_tokens);
    try std.testing.expectEqual(@as(u64, 15), result.token_count);
}
