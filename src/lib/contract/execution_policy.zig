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
    /// Resolved inference endpoint HOST (e.g. "api.fireworks.ai"); "" = none.
    /// Control-plane-authored from the SAME provider→URL table the engine dials
    /// (`nullclaw.providers.compatibleProviderUrl`), so the runner's egress
    /// allowlist permits exactly the host the agent's LLM call will reach — no
    /// drift. Additive + defaulted so old/new leases stay parseable both ways.
    inference_host: []const u8 = "",
    context: ContextBudget = .{},
};

/// Extract the bare host from a provider base URL (`https://api.fireworks.ai/
/// inference/v1` → `api.fireworks.ai`). Returns "" when `url` has no
/// recognizable authority. Pure; used control-plane-side to author
/// `inference_host` and reusable for any host-from-URL need on the contract.
pub fn hostFromUrl(url: []const u8) []const u8 {
    const after_scheme = if (std.mem.indexOf(u8, url, "://")) |i| url[i + 3 ..] else url;
    const authority_end = std.mem.indexOfAny(u8, after_scheme, "/?#") orelse after_scheme.len;
    const authority = after_scheme[0..authority_end];
    // Strip optional userinfo@ and :port — a hostname carries neither.
    const after_userinfo = if (std.mem.lastIndexOfScalar(u8, authority, '@')) |i| authority[i + 1 ..] else authority;
    const host_end = std.mem.indexOfScalar(u8, after_userinfo, ':') orelse after_userinfo.len;
    return after_userinfo[0..host_end];
}

test "hostFromUrl extracts the bare host across URL shapes" {
    try std.testing.expectEqualStrings("api.fireworks.ai", hostFromUrl("https://api.fireworks.ai/inference/v1"));
    try std.testing.expectEqualStrings("api.x.ai", hostFromUrl("https://api.x.ai"));
    try std.testing.expectEqualStrings("api.openai.com", hostFromUrl("https://api.openai.com:443/v1"));
    try std.testing.expectEqualStrings("host", hostFromUrl("host")); // schemeless
    try std.testing.expectEqualStrings("", hostFromUrl("")); // empty → no host
}
