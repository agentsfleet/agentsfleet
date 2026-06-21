//! Unit tests for the engine's usage-split mapping — the seam where the
//! vendored fleet's cumulative accessors become `ExecutionResult` splits.
//! Pins the verbatim mapping (prompt → input, completion → output) and the
//! cached-input-stays-zero limitation, so a pin bump that changes accessor
//! semantics fails here instead of in billing.

const std = @import("std");
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const providers = nullclaw.providers;
const Fleet = nullclaw.agent.Agent;

const runner = @import("runner.zig");
const runner_progress = @import("runner_progress.zig");
const pipe_proto = @import("../pipe_proto.zig");
const clock = @import("common").clock;
const contract = @import("contract");

/// Field-literal Fleet mirroring the upstream agent module's own tokens test:
/// `.provider = undefined` is never invoked (no run call in these tests), and
/// `deinit` owns the zero-length tool_specs allocation.
fn testFleet(allocator: std.mem.Allocator, noop: *observability.NoopObserver) !Fleet {
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

test "usageSplits maps the fleet's cumulative accessors verbatim" {
    var noop = observability.NoopObserver{};
    var fleet = try testFleet(std.testing.allocator, &noop);
    defer fleet.deinit();

    // A fresh fleet maps to all-zero splits.
    const zero = runner.usageSplits(&fleet);
    try std.testing.expectEqual(@as(u64, 0), zero.input);
    try std.testing.expectEqual(@as(u64, 0), zero.output);

    fleet.prompt_tokens_total = 10;
    fleet.completion_tokens_total = 5;
    fleet.total_tokens = 15;

    const splits = runner.usageSplits(&fleet);
    try std.testing.expectEqual(@as(u64, 10), splits.input);
    try std.testing.expectEqual(@as(u64, 5), splits.output);
    // The legacy total rides beside the splits, unchanged.
    try std.testing.expectEqual(@as(u64, 15), fleet.tokensUsed());
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

test "a tokens_used metric emits one usage frame carrying the fleet's cumulative splits" {
    var noop = observability.NoopObserver{};
    var fleet = try testFleet(std.testing.allocator, &noop);
    defer fleet.deinit();
    fleet.prompt_tokens_total = 10;
    fleet.completion_tokens_total = 5;

    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = std.testing.allocator };
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = std.testing.allocator, .secrets = &.{} };
    adapter.fleet = &fleet;

    // The fleet fires this metric after each turn/summary fold — drive it the
    // same way through the observer vtable.
    const obs = adapter.observer();
    const metric = observability.ObserverMetric{ .tokens_used = 15 };
    obs.recordMetric(&metric);
    pipe_proto.testOsClose(fds[1]);

    const out = try pipe_proto.readFrame(std.testing.allocator, fds[0], clock.nowMillis() + 5_000, 1024);
    defer std.testing.allocator.free(out.frame.payload);
    try std.testing.expectEqual(pipe_proto.FrameType.usage, out.frame.ftype);
    const snap = pipe_proto.UsageSnapshot.decode(out.frame.payload).?;
    try std.testing.expectEqual(@as(u64, 10), snap.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), snap.cached_input_tokens); // pinned 0 until upstream surfaces it
    try std.testing.expectEqual(@as(u64, 5), snap.output_tokens);
}

test "usage emit is a no-op without a Fleet pointer (tests and non-streaming paths)" {
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = std.testing.allocator };
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = std.testing.allocator, .secrets = &.{} };

    const obs = adapter.observer();
    const metric = observability.ObserverMetric{ .tokens_used = 15 };
    obs.recordMetric(&metric); // fleet == null → nothing written
    pipe_proto.testOsClose(fds[1]);

    const out = try pipe_proto.readFrame(std.testing.allocator, fds[0], clock.nowMillis() + 5_000, 1024);
    try std.testing.expect(out == .eof); // clean EOF, no frame
}
