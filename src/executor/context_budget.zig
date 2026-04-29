//! Per-execution policy bundle: what the worker passes into
//! `createExecution`, what the session stores, what the tool layer
//! consults on every outbound call. Three concerns travel together
//! because they're all set once per execution and invariant for its
//! lifetime: network allowlist, tool allowlist, secrets_map, and the
//! context-budget knobs.
//!
//! `model` and `context_cap_tokens` originate in the zombie's runtime
//! config (frontmatter for platform-managed credentials, BYOK
//! credential body for customer-supplied providers). Keeping them in
//! the config rather than a compile-time platform table avoids two
//! hazards: a release-train coupling on every new model, and an
//! inability to handle BYOK providers we've never heard of. Tier math
//! is pure (`tierFor` / `defaultToolWindow`) — no model-name dictionary
//! to rot.

const std = @import("std");

/// Per-execution network egress policy. Outbound HTTPS requests must
/// match at least one entry in `allow` (exact hostname match — wildcard
/// support is a follow-up). Empty `allow` means deny-all for this
/// session.
pub const NetworkPolicy = struct {
    allow: []const []const u8 = &.{},
};

/// Context-budget knobs from `x-usezombie.context`. `tool_window == 0`
/// is the sentinel for "auto" — resolved via `defaultToolWindow`
/// against `context_cap_tokens` at use time.
pub const ContextBudget = struct {
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 5,
    stage_chunk_threshold: f32 = 0.75,
    model: []const u8 = "",
    /// Hard cap on input tokens for the resolved model. Drives the
    /// stage-chunk threshold (`current_tokens / context_cap_tokens`
    /// against `stage_chunk_threshold`). 0 means "unspecified" — callers
    /// should fall back to `fallback_cap_tokens` rather than reject the
    /// config.
    context_cap_tokens: u32 = 0,
};

/// Conservative default when a zombie's config omits
/// `context_cap_tokens`. Picked at the lower-medium boundary so an
/// unknown model gets a small, safe tool_window rather than running
/// away with an unbounded budget.
pub const fallback_cap_tokens: u32 = 200_000;

pub const Tier = enum { large, medium, small };

/// Pure math: cap → tier. Boundaries: ≥1M = large, ≥200k = medium,
/// else small.
pub fn tierFor(cap_tokens: u32) Tier {
    if (cap_tokens >= 1_000_000) return .large;
    if (cap_tokens >= 200_000) return .medium;
    return .small;
}

/// Default `tool_window` count when the per-zombie config sets it to
/// `"auto"`. Large windows for big-cap models so long incidents fit;
/// smaller windows for narrow-cap models so older tool results drop out
/// before the agent runs out of room.
pub fn defaultToolWindow(cap_tokens: u32) u32 {
    return switch (tierFor(cap_tokens)) {
        .large => 30,
        .medium => 20,
        .small => 10,
    };
}

/// Bundle of per-execution policy fields set at `createExecution` and
/// invariant for the session's lifetime. Stored on the session,
/// threaded through every tool invocation. Empty defaults are deny-all
/// egress, no tool restriction, no secrets, default context budget —
/// so tests that don't care can pass `.{}`.
pub const ExecutionPolicy = struct {
    network_policy: NetworkPolicy = .{},
    /// Allowlist of tool names the agent may invoke. Empty = no filter
    /// applied (the runtime's default tool registry is used as-is).
    tools: []const []const u8 = &.{},
    /// Resolved credentials — a JSON object whose keys are credential
    /// names and whose values are the parsed JSON bodies stored in
    /// vault. `null` is "no secrets". The substitution path looks up
    /// `${secrets.NAME.FIELD}` against this object at outbound-request
    /// time.
    secrets_map: ?std.json.Value = null,
    context: ContextBudget = .{},
};

test "tierFor partitions caps at 1M and 200k" {
    try std.testing.expectEqual(Tier.large, tierFor(1_000_000));
    try std.testing.expectEqual(Tier.large, tierFor(2_000_000));
    try std.testing.expectEqual(Tier.medium, tierFor(999_999));
    try std.testing.expectEqual(Tier.medium, tierFor(200_000));
    try std.testing.expectEqual(Tier.small, tierFor(199_999));
    try std.testing.expectEqual(Tier.small, tierFor(0));
}

test "defaultToolWindow picks 30 / 20 / 10 by tier" {
    try std.testing.expectEqual(@as(u32, 30), defaultToolWindow(1_000_000));
    try std.testing.expectEqual(@as(u32, 20), defaultToolWindow(200_000));
    try std.testing.expectEqual(@as(u32, 10), defaultToolWindow(50_000));
}

test "defaultToolWindow on the fallback cap is the medium default" {
    try std.testing.expectEqual(@as(u32, 20), defaultToolWindow(fallback_cap_tokens));
}
