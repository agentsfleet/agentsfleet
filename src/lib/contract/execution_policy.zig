//! Per-execution policy types — the data shapes set once per execution and
//! invariant for its lifetime: network allowlist, tool allowlist, secrets_map,
//! and context-budget knobs.
//!
//! Pure data, std-only — no JSON/wire parsing here (that is `context_budget.zig`,
//! which re-exports these). Kept dependency-free so the frozen `/v1/runners`
//! protocol can reuse `ExecutionPolicy` on the lease without dragging runner
//! parsing internals onto the public wire surface. Shared as a named module by
//! both `zombied` and the runner; migrates with the engine to the runner at the
//! cutover.

const std = @import("std");

/// Per-execution network egress policy. Outbound HTTPS requests must match at
/// least one entry in `allow` (exact hostname match). Empty `allow` = deny-all.
pub const NetworkPolicy = struct {
    allow: []const []const u8 = &.{},
};

/// Context-budget knobs from `x-usezombie.context`. `model` + `context_cap_tokens`
/// are upstream-populated passthrough; the runner does not interpret `model`.
pub const ContextBudget = struct {
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 5,
    stage_chunk_threshold: f32 = 0.75,
    model: []const u8 = "",
    context_cap_tokens: u32 = 0,

    /// Substitute defaults for any zero-value (auto-sentinel) field. Mutates in
    /// place. Non-zero fields are operator overrides and left alone. `model` and
    /// `context_cap_tokens` are upstream-populated and don't auto-default.
    pub fn applyDefaults(self: *ContextBudget) void {
        if (self.tool_window == 0) self.tool_window = DEFAULT_TOOL_WINDOW;
        if (self.memory_checkpoint_every == 0) self.memory_checkpoint_every = DEFAULT_MEMORY_CHECKPOINT_EVERY;
        if (self.stage_chunk_threshold == 0.0) self.stage_chunk_threshold = DEFAULT_STAGE_CHUNK_THRESHOLD;
    }
};

/// Auto default for `tool_window` — sized for a 200k-class working set.
pub const DEFAULT_TOOL_WINDOW: u32 = 20;
/// Auto default for `memory_checkpoint_every` — cheap and always safe.
pub const DEFAULT_MEMORY_CHECKPOINT_EVERY: u32 = 5;
/// Auto default for `stage_chunk_threshold` — L3 failsafe at 75% fill.
pub const DEFAULT_STAGE_CHUNK_THRESHOLD: f32 = 0.75;

/// Bundle of per-execution policy fields, set at `createExecution` and invariant
/// for the session's lifetime. Empty defaults: deny-all egress, no tool filter,
/// no secrets, default context budget. Parsing lives in `context_budget.fromJson`.
pub const ExecutionPolicy = struct {
    network_policy: NetworkPolicy = .{},
    /// Allowlist of tool names the agent may invoke. Empty = no filter.
    tools: []const []const u8 = &.{},
    /// Resolved credentials — JSON object keyed by credential name, values the
    /// parsed JSON bodies from vault. `null` = no secrets. Substitution looks up
    /// `${secrets.NAME.FIELD}` against this at outbound-request time.
    secrets_map: ?std.json.Value = null,
    /// Resolved LLM provider name for this lease (e.g. "fireworks"); "" = none.
    /// Authoritative — the engine authenticates against the same provider the
    /// tenant is billed for. Carried inline on the lease, additive + defaulted
    /// so old/new leases stay parseable both ways.
    provider: []const u8 = "",
    /// Resolved LLM api_key for `provider`; "" = none. Sensitive inline secret:
    /// never logged, never persisted to the lease row, redacted from activity
    /// frames (engine keys redaction off `agent_config.api_key`).
    api_key: []const u8 = "",
    context: ContextBudget = .{},
};
