//! Compile-time registry of LLM model identifiers and their context-window
//! capacities. Drives two M41 paths:
//!
//!   - §6 stage_chunk_threshold: `current_tokens / context_cap_tokens` is
//!     the ratio compared against the budget threshold.
//!   - §8 auto-defaults: when `context.tool_window == "auto"`, the resolved
//!     model's tier picks a sensible default.
//!
//! The table is intentionally a static `const` array rather than a hashmap.
//! Registry lookups happen once per `createExecution` over a handful of
//! entries — linear scan beats hashmap setup cost. New model rows land via
//! PR (admin: nkishore@megam.io); when M48 (BYOK) ships, tenant-supplied
//! model names are validated against this same table.

const std = @import("std");

pub const Entry = struct {
    name: []const u8,
    context_cap_tokens: u32,
};

pub const Tier = enum { large, medium, small };

/// Anthropic models currently supported by the runtime. Extend by PR when a
/// new model rolls out; M48 will layer tenant-validation on the same set.
pub const entries = [_]Entry{
    .{ .name = "claude-opus-4-7", .context_cap_tokens = 1_000_000 },
    .{ .name = "claude-sonnet-4-6", .context_cap_tokens = 200_000 },
    .{ .name = "claude-sonnet-4-5", .context_cap_tokens = 200_000 },
    .{ .name = "claude-haiku-4-5-20251001", .context_cap_tokens = 200_000 },
};

pub const UnknownModelError = error{UnknownModel};

pub fn lookup(name: []const u8) UnknownModelError!Entry {
    for (entries) |e| {
        if (std.mem.eql(u8, e.name, name)) return e;
    }
    return error.UnknownModel;
}

/// Tier breakpoints. Boundary policy: ≥ 1M → large, ≥ 200k → medium,
/// otherwise small. The 200k boundary is inclusive on the medium side
/// because every shipped 200k-cap model in this registry should pick the
/// medium default tool window, not the small one.
pub fn tierFor(cap_tokens: u32) Tier {
    if (cap_tokens >= 1_000_000) return .large;
    if (cap_tokens >= 200_000) return .medium;
    return .small;
}

/// Default `tool_window` count when the per-zombie config sets it to
/// `"auto"`. Numbers come from M41 §8 — large windows for big-cap models so
/// long incidents fit, smaller windows for narrow-cap models so older tool
/// results drop out before the agent runs out of room.
pub fn defaultToolWindow(cap_tokens: u32) u32 {
    return switch (tierFor(cap_tokens)) {
        .large => 30,
        .medium => 20,
        .small => 10,
    };
}

test "lookup resolves shipped Anthropic models" {
    const opus = try lookup("claude-opus-4-7");
    try std.testing.expectEqual(@as(u32, 1_000_000), opus.context_cap_tokens);

    const sonnet = try lookup("claude-sonnet-4-6");
    try std.testing.expectEqual(@as(u32, 200_000), sonnet.context_cap_tokens);
}

test "lookup returns UnknownModel for unregistered names" {
    try std.testing.expectError(error.UnknownModel, lookup("gpt-5"));
    try std.testing.expectError(error.UnknownModel, lookup(""));
    try std.testing.expectError(error.UnknownModel, lookup("claude-opus-4-6"));
}

test "tierFor partitions caps at 1M and 200k" {
    try std.testing.expectEqual(Tier.large, tierFor(1_000_000));
    try std.testing.expectEqual(Tier.large, tierFor(2_000_000));
    try std.testing.expectEqual(Tier.medium, tierFor(999_999));
    try std.testing.expectEqual(Tier.medium, tierFor(200_000));
    try std.testing.expectEqual(Tier.small, tierFor(199_999));
    try std.testing.expectEqual(Tier.small, tierFor(0));
}

test "defaultToolWindow follows the §8 table" {
    try std.testing.expectEqual(@as(u32, 30), defaultToolWindow(1_000_000));
    try std.testing.expectEqual(@as(u32, 20), defaultToolWindow(200_000));
    try std.testing.expectEqual(@as(u32, 10), defaultToolWindow(50_000));
}

test "every shipped entry has a non-zero cap" {
    for (entries) |e| {
        try std.testing.expect(e.context_cap_tokens > 0);
        try std.testing.expect(e.name.len > 0);
    }
}
