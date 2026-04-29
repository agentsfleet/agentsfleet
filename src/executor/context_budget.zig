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

/// Context-budget knobs from `x-usezombie.context`. Slice §1 plumbs
/// these through the wire; slice §6 picks how to interpret them when
/// it implements the L3 stage-chunk threshold.
pub const ContextBudget = struct {
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 5,
    stage_chunk_threshold: f32 = 0.75,
    /// Free-form model identifier from the zombie's runtime config
    /// (e.g. "claude-opus-4-7"). The executor doesn't interpret this —
    /// it flows through for observability and is consumed by NullClaw
    /// when it talks to the LLM provider.
    model: []const u8 = "",
};

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
