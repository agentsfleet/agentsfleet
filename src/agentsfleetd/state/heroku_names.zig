//! Heroku-style workspace name generator: `{adjective}-{noun}-{NNN}`.
//!
//! The signup bootstrap assigns every new personal workspace a memorable
//! default name (e.g. `jolly-harbor-482`). Word lists are small and inlined —
//! collision avoidance happens at the SQL layer via `uq_workspaces_tenant_name`,
//! not via list cardinality. Pure: no DB, OOM is the only error.
//! Randomness is `common.secureRandomBytes` (modulo reduction — bias is
//! negligible for these tiny word-list ranges, and names aren't security state).

const std = @import("std");
const constants = @import("common");

pub const ADJECTIVES = [_][]const u8{
    "jolly",  "bright", "swift",   "calm",
    "lively", "bold",   "silent",  "happy",
    "gentle", "brave",  "sunny",   "mellow",
    "eager",  "keen",   "plucky",  "hardy",
    "dandy",  "spry",   "nimble",  "zesty",
    "peppy",  "witty",  "hearty",  "cosy",
    "dreamy", "fuzzy",  "rustic",  "mossy",
    "breezy", "tidy",   "stellar", "cozy",
};

pub const NOUNS = [_][]const u8{
    "harbor",  "forest",  "river",   "meadow",
    "canyon",  "island",  "glacier", "valley",
    "summit",  "lagoon",  "ridge",   "plateau",
    "orchard", "prairie", "delta",   "bayou",
    "cove",    "reef",    "basin",   "grove",
    "gulch",   "fjord",   "marsh",   "mesa",
    "atoll",   "knoll",   "tundra",  "brook",
    "thicket", "shore",   "mount",   "brae",
};

/// 3-digit zero-padded suffix keeps the name visually consistent
/// (`jolly-harbor-042`, not `jolly-harbor-42`).
pub const SUFFIX_MAX: u32 = 1000;

/// Generate a fresh `{adjective}-{noun}-{NNN}` name. Caller owns the slice.
pub fn generate(alloc: std.mem.Allocator) ![]u8 {
    var rb: [12]u8 = undefined;
    try constants.secureRandomBytes(&rb);
    const adj_idx = std.mem.readInt(u32, rb[0..4], .little) % @as(u32, ADJECTIVES.len);
    const noun_idx = std.mem.readInt(u32, rb[4..8], .little) % @as(u32, NOUNS.len);
    const suffix = std.mem.readInt(u32, rb[8..12], .little) % SUFFIX_MAX;
    return std.fmt.allocPrint(alloc, "{s}-{s}-{d:0>3}", .{
        ADJECTIVES[adj_idx],
        NOUNS[noun_idx],
        suffix,
    });
}

/// Collision breaker for a taken DEFAULT name: `{base}-{NNN}` under `max_len`,
/// truncating the base (never the random tail) when it would not fit, and
/// never leaving a trailing `-` before the tail. Used by fleet install when
/// the operator named nothing and the template's name is already running in
/// the workspace — the same random tail the workspace names carry, so one
/// naming mechanism serves both. Caller owns the slice.
pub fn suffixed(alloc: std.mem.Allocator, base: []const u8, max_len: usize) ![]u8 {
    var rb: [4]u8 = undefined;
    try constants.secureRandomBytes(&rb);
    const tail = std.mem.readInt(u32, rb[0..4], .little) % SUFFIX_MAX;
    const reserve = 4; // "-NNN"
    var keep = @min(base.len, max_len -| reserve);
    while (keep > 0 and base[keep - 1] == '-') keep -= 1;
    return std.fmt.allocPrint(alloc, "{s}-{d:0>3}", .{ base[0..keep], tail });
}
