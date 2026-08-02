//! The error registry's machinery: the assembled entry table, its comptime
//! validation, and the code lookup.
//!
//! Split out of `error_registry.zig` so that file can hold what only it may
//! hold. The error-codes audit greps `error_registry.zig` alone for declared
//! codes, so every `UZ-*` literal has to live there and nowhere else — which
//! means that file grows with every new error family and cannot be relieved by
//! moving codes out. Everything that is NOT a code declaration can leave, and
//! this is the largest such piece. `error_registry.zig` re-exports all of it,
//! so no call site knows the difference.

const std = @import("std");
const entries = @import("error_entries.zig");
const entries_runtime = @import("error_entries_runtime.zig");

pub const Entry = entries.Entry;
pub const UNKNOWN = entries.UNKNOWN;
pub const ERROR_DOCS_BASE = entries.ERROR_DOCS_BASE;
pub const REGISTRY = entries.ENTRIES ++ entries_runtime.ENTRIES_RUNTIME;

comptime {
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 20);
    for (REGISTRY) |entry| {
        if (entry.hint.len == 0)
            @compileError("Entry has empty hint: " ++ entry.code);
        if (entry.code.len < 4 or !std.mem.startsWith(u8, entry.code, "UZ-"))
            @compileError("Entry code must start with UZ-: " ++ entry.code);
        if (entry.user_message) |um| {
            if (um.len == 0)
                @compileError("Entry has empty user_message (omit the field instead of authoring an empty string): " ++ entry.code);
        }
    }
    // Invariant 3: no sentinel collision
    for (REGISTRY) |entry| {
        if (std.mem.eql(u8, entry.code, UNKNOWN.code))
            @compileError("REGISTRY entry collides with UNKNOWN sentinel: " ++ entry.code);
    }
    // Invariant 5: no duplicate codes
    for (REGISTRY, 0..) |a, i| {
        for (REGISTRY[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a.code, b.code))
                @compileError("Duplicate code in REGISTRY: " ++ a.code);
        }
    }
}

const LOOKUP = blk: {
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 20);
    var kvs: [REGISTRY.len]struct { []const u8, usize } = undefined;
    for (REGISTRY, 0..) |entry, i| kvs[i] = .{ entry.code, i };
    break :blk std.StaticStringMap(usize).initComptime(kvs);
};

/// Lookup by code string. Returns UNKNOWN for unregistered codes.
/// Never returns null — callers do not need optional handling.
pub fn lookup(code: []const u8) Entry {
    const idx = LOOKUP.get(code) orelse return UNKNOWN;
    return REGISTRY[idx];
}

/// Lookup hint for an error code. Returns UNKNOWN.hint for unregistered codes.
pub fn hint(code: []const u8) []const u8 {
    return lookup(code).hint;
}

/// Whether a code has a registry entry. Used by `error_registry.zig`'s comptime
/// self-check, which cannot reach `LOOKUP` from outside this file.
pub fn isRegistered(code: []const u8) bool {
    return LOOKUP.get(code) != null;
}
