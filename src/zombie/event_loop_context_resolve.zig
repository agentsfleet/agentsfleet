//! Resolve the executor's `ContextBudget` from a parsed `ZombieConfig`.
//!
//! Extracted from `event_loop_helpers.zig` so that file stays under the
//! 350-line cap. The wiring is small (frontmatter overrides → executor
//! struct → auto-defaults) but it's load-bearing — every event-loop turn
//! goes through it — and folding it into a separate file lets the
//! `applyContextDefaults` invariant tests stay close to the resolution
//! logic that depends on them.

const std = @import("std");
const context_budget = @import("../executor/context_budget.zig");
const config_types = @import("config_types.zig");

/// Build a fully-resolved `ContextBudget` from the zombie's parsed config.
/// Frontmatter overrides win; absent / zero fields fall through to
/// `applyContextDefaults`. `model` is opaque pass-through.
pub fn resolveContextBudget(
    config_ctx: ?config_types.ZombieContextBudget,
    config_model: ?[]const u8,
) context_budget.ContextBudget {
    var ctx: context_budget.ContextBudget = .{};
    if (config_ctx) |c| {
        ctx.context_cap_tokens = c.context_cap_tokens;
        ctx.tool_window = c.tool_window;
        ctx.memory_checkpoint_every = c.memory_checkpoint_every;
        ctx.stage_chunk_threshold = c.stage_chunk_threshold;
    }
    if (config_model) |m| ctx.model = m;
    context_budget.applyContextDefaults(&ctx);
    return ctx;
}

test "resolveContextBudget: null config falls through to auto-defaults" {
    const ctx = resolveContextBudget(null, null);
    try std.testing.expectEqual(context_budget.DEFAULT_TOOL_WINDOW, ctx.tool_window);
    try std.testing.expectEqual(context_budget.DEFAULT_MEMORY_CHECKPOINT_EVERY, ctx.memory_checkpoint_every);
    try std.testing.expectEqual(context_budget.DEFAULT_STAGE_CHUNK_THRESHOLD, ctx.stage_chunk_threshold);
    try std.testing.expectEqual(@as(u32, 0), ctx.context_cap_tokens);
    try std.testing.expectEqualStrings("", ctx.model);
}

test "resolveContextBudget: frontmatter overrides win against auto-defaults" {
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 256000,
        .tool_window = 30,
        .memory_checkpoint_every = 7,
        .stage_chunk_threshold = 0.6,
    }, "kimi-k2.6");
    try std.testing.expectEqual(@as(u32, 256000), ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 30), ctx.tool_window);
    try std.testing.expectEqual(@as(u32, 7), ctx.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.6), ctx.stage_chunk_threshold);
    try std.testing.expectEqualStrings("kimi-k2.6", ctx.model);
}

test "resolveContextBudget: zero-valued frontmatter knobs still default (BYOK sentinel path)" {
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 0,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "");
    try std.testing.expectEqual(@as(u32, 0), ctx.context_cap_tokens);
    try std.testing.expectEqual(context_budget.DEFAULT_TOOL_WINDOW, ctx.tool_window);
    try std.testing.expectEqualStrings("", ctx.model);
}
