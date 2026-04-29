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
