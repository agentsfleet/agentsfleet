//! Per-execution policy parsing. The policy *types* live in the dependency-free
//! `execution_policy` module (shared with the `/v1/runners` protocol so the
//! frozen wire surface needn't import executor parsing internals); this file
//! re-exports them so existing `context_budget.X` call sites stay stable, and
//! owns the JSON/wire parsing (`fromJson`) the executor engine needs.

const std = @import("std");
const json = @import("json_helpers.zig");
const wire = @import("wire.zig");
const execution_policy = @import("execution_policy.zig");

// Re-export the policy types — existing executor call sites use `context_budget.X`.
pub const NetworkPolicy = execution_policy.NetworkPolicy;
pub const ContextBudget = execution_policy.ContextBudget;
pub const ExecutionPolicy = execution_policy.ExecutionPolicy;
pub const DEFAULT_TOOL_WINDOW = execution_policy.DEFAULT_TOOL_WINDOW;
pub const DEFAULT_MEMORY_CHECKPOINT_EVERY = execution_policy.DEFAULT_MEMORY_CHECKPOINT_EVERY;
pub const DEFAULT_STAGE_CHUNK_THRESHOLD = execution_policy.DEFAULT_STAGE_CHUNK_THRESHOLD;

/// Parse the per-execution policy fields off CreateExecution params into a
/// borrowed `ExecutionPolicy`. Allocates string slices on `alloc` (the request
/// frame allocator); `Session.create` deep-dupes into its own arena before the
/// frame ends.
pub fn fromJson(alloc: std.mem.Allocator, p: std.json.Value) !ExecutionPolicy {
    if (p != .object) return error.NotAnObject;
    var policy: ExecutionPolicy = .{};

    if (p.object.get(wire.network_policy)) |np_val| {
        if (np_val == .object) {
            if (np_val.object.get(wire.allow)) |allow_val| {
                policy.network_policy = .{ .allow = try json.strArray(alloc, allow_val) };
            }
        }
    }

    if (p.object.get(wire.tools)) |tools_val| {
        policy.tools = try json.strArray(alloc, tools_val);
    }

    if (p.object.get(wire.secrets_map)) |sm_val| {
        if (sm_val == .object) policy.secrets_map = sm_val;
    }

    if (p.object.get(wire.context)) |ctx_val| {
        if (ctx_val == .object) {
            if (ctx_val.object.get(wire.tool_window)) |tw| {
                if (tw == .integer and tw.integer >= 0)
                    policy.context.tool_window = @intCast(tw.integer);
            }
            if (ctx_val.object.get(wire.memory_checkpoint_every)) |mce| {
                if (mce == .integer and mce.integer > 0)
                    policy.context.memory_checkpoint_every = @intCast(mce.integer);
            }
            if (ctx_val.object.get(wire.stage_chunk_threshold)) |sct| {
                policy.context.stage_chunk_threshold = switch (sct) {
                    .float => |f| @floatCast(f),
                    .integer => |i| @floatFromInt(i),
                    else => policy.context.stage_chunk_threshold,
                };
            }
            if (json.getStr(ctx_val, wire.model)) |m| policy.context.model = m;
            if (ctx_val.object.get(wire.context_cap_tokens)) |cct| {
                if (cct == .integer and cct.integer >= 0)
                    policy.context.context_cap_tokens = @intCast(cct.integer);
            }
        }
    }

    return policy;
}

test "ContextBudget.applyDefaults substitutes the three auto-sentinel knobs" {
    var cb: ContextBudget = .{ .tool_window = 0, .memory_checkpoint_every = 0, .stage_chunk_threshold = 0.0 };
    cb.applyDefaults();
    try std.testing.expectEqual(DEFAULT_TOOL_WINDOW, cb.tool_window);
    try std.testing.expectEqual(DEFAULT_MEMORY_CHECKPOINT_EVERY, cb.memory_checkpoint_every);
    try std.testing.expectEqual(DEFAULT_STAGE_CHUNK_THRESHOLD, cb.stage_chunk_threshold);
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
