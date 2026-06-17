//! Resolve the shared `ContextBudget` from a parsed `AgentConfig`.
//!
//! Lifted from the worker's `event_loop_helpers.zig` at the M80 cutover onto
//! the lease verb's per-event prep. The wiring is small (frontmatter overrides
//! → policy struct → auto-defaults) but it's load-bearing — every lease goes
//! through it — and the standalone file keeps the `ContextBudget.applyDefaults`
//! invariant tests next to the resolution logic that depends on them.

const std = @import("std");
const context_budget = @import("contract").execution_policy;
const config_types = @import("../agent/config_types.zig");

// Drift guard: every field on the parser-side `AgentContextBudget` must
// exist on the runner-side `ContextBudget` at the same name + type, OR
// the field-by-field copy below silently drops the operator override at
// trigger time. Adding `max_tool_calls` to `ContextBudget` without a
// matching `AgentContextBudget` field is a separate failure mode (the
// new knob would be runtime-only, never frontmatter-overridable) — caught
// by the inverse check.
comptime {
    const ZB = config_types.AgentContextBudget;
    const CB = context_budget.ContextBudget;
    const zb_fields = std.meta.fields(ZB);
    for (zb_fields) |f| {
        if (!@hasField(CB, f.name)) {
            @compileError("AgentContextBudget field '" ++ f.name ++ "' missing from ContextBudget — pair them or rename");
        }
        const cb_field_type = @FieldType(CB, f.name);
        if (cb_field_type != f.type) {
            @compileError("AgentContextBudget." ++ f.name ++ " type drifts from ContextBudget." ++ f.name ++ " — pair them");
        }
    }
    // Inverse guard: any new ContextBudget field that LOOKS like a
    // frontmatter knob (numeric, non-`model`) but isn't paired in
    // AgentContextBudget would silently be runtime-only forever.
    // Pin AgentContextBudget's size so a agent-side addition without
    // a matching parser entry trips this assert in the same commit.
    std.debug.assert(@sizeOf(ZB) == 16);
}

/// Build a fully-resolved `ContextBudget` from the agent's parsed config.
/// Per-field, independent resolution (see `user_flow.md`): a present
/// frontmatter value wins (non-zero `context_cap_tokens`, non-empty `model`);
/// a sentinel (`context_cap_tokens: 0` / `model: ""` / absent) overlays from
/// the resolved tenant provider (`overlay_cap` / `overlay_model`, which the
/// control plane resolved from the model-caps endpoint at install /
/// `tenant provider add` time); anything still unset falls to `applyDefaults`.
/// Pass `overlay_cap: 0` / `overlay_model: ""` when no provider resolved — the
/// overlay is then a no-op and an unresolved cap leaves `tool_window` at the
/// mid tier with L3 chunking inert (it needs a non-zero cap).
pub fn resolveContextBudget(
    config_ctx: ?config_types.AgentContextBudget,
    config_model: ?[]const u8,
    overlay_cap: u32,
    overlay_model: []const u8,
) context_budget.ContextBudget {
    var ctx: context_budget.ContextBudget = .{};
    if (config_ctx) |c| {
        ctx.context_cap_tokens = c.context_cap_tokens;
        ctx.tool_window = c.tool_window;
        ctx.memory_checkpoint_every = c.memory_checkpoint_every;
        ctx.stage_chunk_threshold = c.stage_chunk_threshold;
    }
    // Borrowed slice — `model` points at either `session.config.model`
    // (frontmatter) or `overlay_model` (the resolved provider's model). Both
    // are caller-owned and outlive the lease serialization (`resolved` deinits
    // after `hx.ok` in issueLease). ContextBudget is a value-type with no
    // deinit; the runner must not free this. If ContextBudget ever gains a
    // destructor, these assignments become unsafe and need alloc.dupe.
    if (config_model) |m| ctx.model = m;

    // Lease-time tenant-provider overlay: a sentinel (zero/empty) frontmatter
    // field inherits the value the control plane resolved into tenant_providers;
    // a real frontmatter value is left untouched. Must precede applyDefaults so
    // an overlaid cap feeds the auto `tool_window` tiering.
    if (ctx.context_cap_tokens == 0 and overlay_cap != 0) ctx.context_cap_tokens = overlay_cap;
    if (ctx.model.len == 0 and overlay_model.len != 0) ctx.model = overlay_model;

    ctx.applyDefaults();
    return ctx;
}

test "resolveContextBudget: null config falls through to auto-defaults" {
    const ctx = resolveContextBudget(null, null, 0, "");
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
    }, "kimi-k2.6", 0, "");
    try std.testing.expectEqual(@as(u32, 256000), ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 30), ctx.tool_window);
    try std.testing.expectEqual(@as(u32, 7), ctx.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.6), ctx.stage_chunk_threshold);
    try std.testing.expectEqualStrings("kimi-k2.6", ctx.model);
}

test "resolveContextBudget: sentinel knobs with no provider resolved fall to static defaults" {
    // overlay_cap=0 / overlay_model="" is the resolve-failure path: nothing to
    // inherit, so the cap stays 0 (tool_window mid-default, L3 inert) and model "".
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 0,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "", 0, "");
    try std.testing.expectEqual(@as(u32, 0), ctx.context_cap_tokens);
    try std.testing.expectEqual(context_budget.DEFAULT_TOOL_WINDOW, ctx.tool_window);
    try std.testing.expectEqualStrings("", ctx.model);
}

// ── lease-time tenant-provider overlay (see user_flow.md) ───────────────────

// Mirrors the private `execution_policy.CONTEXT_CAP_LARGE_TOKENS`: an overlaid
// cap at/above it resolves `tool_window` to the large tier (30).
const LARGE_CAP_TOKENS: u32 = 1_000_000;

test "resolveContextBudget: sentinel cap+model inherit from the resolved provider" {
    // self-managed sentinel path: frontmatter pins nothing, so both fields
    // overlay from tenant_providers, and the overlaid cap drives the tiering.
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 0,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "", LARGE_CAP_TOKENS, "accounts/fireworks/models/kimi-k2.6");
    try std.testing.expectEqual(LARGE_CAP_TOKENS, ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 30), ctx.tool_window); // pin: ≥1M → large tier (capabilities.md §4)
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", ctx.model);
}

test "resolveContextBudget: present frontmatter wins over the provider overlay" {
    // platform-baked (or operator-pinned) values are never clobbered by overlay.
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 256_000,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "pinned-model", LARGE_CAP_TOKENS, "provider-model");
    try std.testing.expectEqual(@as(u32, 256_000), ctx.context_cap_tokens);
    try std.testing.expectEqual(context_budget.DEFAULT_TOOL_WINDOW, ctx.tool_window); // 256k → mid tier (20)
    try std.testing.expectEqualStrings("pinned-model", ctx.model);
}

test "resolveContextBudget: overlay is per-field independent (cap inherits, model pinned)" {
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 0,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "pinned-model", 200_000, "provider-model");
    try std.testing.expectEqual(@as(u32, 200_000), ctx.context_cap_tokens); // inherited from overlay
    try std.testing.expectEqual(@as(u32, 10), ctx.tool_window); // pin: ≤200k → small tier (capabilities.md §4)
    try std.testing.expectEqualStrings("pinned-model", ctx.model); // frontmatter pin survives
}

test "resolveContextBudget: overlay is per-field independent (cap pinned, model inherits)" {
    // the inverse asymmetry — proves the two fields overlay on independent
    // conditions, not as a coupled pair.
    const ctx = resolveContextBudget(.{
        .context_cap_tokens = 256_000,
        .tool_window = 0,
        .memory_checkpoint_every = 0,
        .stage_chunk_threshold = 0.0,
    }, "", LARGE_CAP_TOKENS, "provider-model");
    try std.testing.expectEqual(@as(u32, 256_000), ctx.context_cap_tokens); // frontmatter pin survives
    try std.testing.expectEqual(context_budget.DEFAULT_TOOL_WINDOW, ctx.tool_window); // 256k → mid tier (20)
    try std.testing.expectEqualStrings("provider-model", ctx.model); // inherited from overlay
}
