//! Per-execution policy bundle: what the worker passes into
//! `createExecution`, what the session stores, what the tool layer
//! consults on every outbound call. Network allowlist, tool allowlist,
//! secrets_map, and the context-budget knobs travel together because
//! they're all set once per execution and invariant for its lifetime.

const std = @import("std");

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

/// Substitute defaults for any zero-value (auto-sentinel) field. Mutates
/// in place. Non-zero fields are operator overrides and left alone.
/// `model` and `context_cap_tokens` are upstream-populated and don't
/// participate in auto-defaulting here — `model` is opaque to this
/// binary, `context_cap_tokens=0` is the install-skill / BYOK signal
/// that L3 short-circuits (capabilities.md §4 escape clause).
pub fn applyContextDefaults(c: *ContextBudget) void {
    if (c.tool_window == 0) c.tool_window = DEFAULT_TOOL_WINDOW;
    if (c.memory_checkpoint_every == 0) c.memory_checkpoint_every = DEFAULT_MEMORY_CHECKPOINT_EVERY;
    if (c.stage_chunk_threshold == 0.0) c.stage_chunk_threshold = DEFAULT_STAGE_CHUNK_THRESHOLD;
}

test "applyContextDefaults substitutes the three auto-sentinel knobs" {
    var cb: ContextBudget = .{ .tool_window = 0, .memory_checkpoint_every = 0, .stage_chunk_threshold = 0.0 };
    applyContextDefaults(&cb);
    try std.testing.expectEqual(DEFAULT_TOOL_WINDOW, cb.tool_window);
    try std.testing.expectEqual(DEFAULT_MEMORY_CHECKPOINT_EVERY, cb.memory_checkpoint_every);
    try std.testing.expectEqual(DEFAULT_STAGE_CHUNK_THRESHOLD, cb.stage_chunk_threshold);
}

test "applyContextDefaults preserves operator overrides" {
    var cb: ContextBudget = .{ .tool_window = 8, .memory_checkpoint_every = 3, .stage_chunk_threshold = 0.6 };
    applyContextDefaults(&cb);
    try std.testing.expectEqual(@as(u32, 8), cb.tool_window);
    try std.testing.expectEqual(@as(u32, 3), cb.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.6), cb.stage_chunk_threshold);
}

test "applyContextDefaults leaves model and context_cap_tokens untouched" {
    var cb: ContextBudget = .{ .model = "claude-sonnet-4-6", .context_cap_tokens = 0 };
    applyContextDefaults(&cb);
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
};
