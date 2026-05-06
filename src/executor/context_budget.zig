//! Per-execution policy bundle: what the worker passes into
//! `createExecution`, what the session stores, what the tool layer
//! consults on every outbound call. Network allowlist, tool allowlist,
//! secrets_map, and the context-budget knobs travel together because
//! they're all set once per execution and invariant for its lifetime.

const std = @import("std");
const json = @import("json_helpers.zig");
const wire = @import("wire.zig");

/// Per-execution network egress policy. Outbound HTTPS requests must
/// match at least one entry in `allow` (exact hostname match — wildcard
/// support is a follow-up). Empty `allow` means deny-all for this
/// session.
pub const NetworkPolicy = struct {
    allow: []const []const u8 = &.{},
};

/// Context-budget knobs from `x-usezombie.context`. M41 plumbs these
/// through the wire; M41's L3 slice (§6) picks how to interpret
/// `stage_chunk_threshold` against `context_cap_tokens` when it lands.
/// Where `context_cap_tokens` comes from is M49's problem: the
/// install-skill writes a sensible value into the SKILL.md frontmatter
/// when it generates the zombie, so the model→cap question gets
/// answered in the install-skill (which can update without a
/// usezombie release) rather than in this binary.
pub const ContextBudget = struct {
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 5,
    stage_chunk_threshold: f32 = 0.75,
    /// Free-form model identifier from the zombie's runtime config
    /// (e.g. "claude-opus-4-7"). Passthrough only — the executor
    /// doesn't interpret it.
    model: []const u8 = "",
    /// Hard cap on input tokens for the resolved model. Populated by
    /// M49 (install-skill) into the zombie's frontmatter, or left at 0
    /// for BYOK flows where the customer sets it on their credential.
    /// Slice §6 decides the fallback when this is 0.
    context_cap_tokens: u32 = 0,

    /// Substitute defaults for any zero-value (auto-sentinel) field. Mutates
    /// in place. Non-zero fields are operator overrides and left alone.
    /// `model` and `context_cap_tokens` are upstream-populated and don't
    /// participate in auto-defaulting here — `model` is opaque to this
    /// binary, `context_cap_tokens=0` is the install-skill / BYOK signal
    /// that L3 short-circuits (capabilities.md §4 escape clause).
    pub fn applyDefaults(self: *ContextBudget) void {
        if (self.tool_window == 0) self.tool_window = DEFAULT_TOOL_WINDOW;
        if (self.memory_checkpoint_every == 0) self.memory_checkpoint_every = DEFAULT_MEMORY_CHECKPOINT_EVERY;
        if (self.stage_chunk_threshold == 0.0) self.stage_chunk_threshold = DEFAULT_STAGE_CHUNK_THRESHOLD;
    }
};

// ── §8 auto-defaults — applied at the worker's createExecution boundary ─────
//
// The wire shape uses 0 / 0.0 as the "auto" sentinel: when the zombie's
// frontmatter doesn't override a knob, the JSON arrives with the field
// at its zero value and the worker substitutes the default below
// before constructing ExecutionPolicy.context. Defaults are
// model-agnostic in this PR — the right cadence + threshold don't
// meaningfully change with context size; tier-aware tool_window
// resolution (30 for ≥1M, 20 for 200-300k, 10 for ≤200k) lands in
// the install-skill, not the runtime.

/// Auto default for `tool_window`. Sized for a 200k-class working set,
/// per spec §8 initial proposal. Customers with bigger or smaller
/// working sets pick a number explicitly via `x-usezombie.context`.
pub const DEFAULT_TOOL_WINDOW: u32 = 20;

/// Auto default for `memory_checkpoint_every`. Cheap and always safe;
/// see capabilities.md L1.
pub const DEFAULT_MEMORY_CHECKPOINT_EVERY: u32 = 5;

/// Auto default for `stage_chunk_threshold`. The L3 failsafe fires at
/// 75% fill of the resolved context cap.
pub const DEFAULT_STAGE_CHUNK_THRESHOLD: f32 = 0.75;

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

/// Bundle of per-execution policy fields set at `createExecution` and
/// invariant for the session's lifetime. Stored on the session,
/// threaded through every tool invocation. Empty defaults are deny-all
/// egress, no tool restriction, no secrets, default context budget —
/// so tests that don't care can pass `.{}`.
pub const ExecutionPolicy = struct {
    network_policy: NetworkPolicy = .{},
    /// Allowlist of tool names the agent may invoke. Empty = no filter.
    tools: []const []const u8 = &.{},
    /// Resolved credentials — a JSON object whose keys are credential
    /// names and whose values are the parsed JSON bodies stored in
    /// vault. `null` is "no secrets". The substitution path looks up
    /// `${secrets.NAME.FIELD}` against this object at outbound-request
    /// time.
    secrets_map: ?std.json.Value = null,
    context: ContextBudget = .{},

    /// Parse the per-execution policy fields off CreateExecution params
    /// into a borrowed `ExecutionPolicy`. Allocates string slices on
    /// `alloc` (the request frame allocator) — the lifetime is bounded by
    /// the frame. `Session.create` deep-dupes into its own arena before
    /// the frame ends.
    pub fn fromJson(alloc: std.mem.Allocator, p: std.json.Value) !ExecutionPolicy {
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
};
