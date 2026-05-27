//! Heroku-style workspace name generator: `{adjective}-{noun}-{NNN}`.
//!
//! The signup bootstrap assigns every new personal workspace a memorable
//! default name (e.g. `jolly-harbor-482`). Word lists are small and inlined —
//! collision avoidance happens at the SQL layer via `uq_workspaces_tenant_name`,
//! not via list cardinality. Pure: no DB, OOM is the only error.
//! Randomness is `std.crypto.random`.

const std = @import("std");

pub const ADJECTIVES = [_][]const u8{
    "jolly",   "bright",  "swift",   "calm",
    "lively",  "bold",    "silent",  "happy",
    "gentle",  "brave",   "sunny",   "mellow",
    "eager",   "keen",    "plucky",  "hardy",
    "dandy",   "spry",    "nimble",  "zesty",
    "peppy",   "witty",   "hearty",  "cosy",
    "dreamy",  "fuzzy",   "rustic",  "mossy",
    "breezy",  "tidy",    "stellar", "cozy",
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
    const adj_idx = std.crypto.random.intRangeLessThan(usize, 0, ADJECTIVES.len);
    const noun_idx = std.crypto.random.intRangeLessThan(usize, 0, NOUNS.len);
    const suffix = std.crypto.random.intRangeLessThan(u32, 0, SUFFIX_MAX);
    return std.fmt.allocPrint(alloc, "{s}-{s}-{d:0>3}", .{
        ADJECTIVES[adj_idx],
        NOUNS[noun_idx],
        suffix,
    });
}
