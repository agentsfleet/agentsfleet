//! Input validation helpers for git runtime-artifact cleanup.
//!
//! All functions are pure (no allocations, no I/O). They are called by
//! `repo.zig` to filter directory entries before any child-process spawn or
//! `deleteTreeAbsolute`, so callers never touch a path that isn't shaped like
//! a zombie-managed artifact.
//!
//! Ownership: no heap allocations — all parameters are borrowed slices.

const std = @import("std");

/// Returns true iff `value` contains only alphanumerics, `-`, `_`, and `.`,
/// and is non-empty. Used as the building-block for workspace-id validation.
pub fn isSafeIdentifierSegment(value: []const u8) bool {
    if (value.len == 0) return false;
    for (value) |c| {
        if (std.ascii.isAlphanumeric(c)) continue;
        if (c == '-' or c == '_' or c == '.') continue;
        return false;
    }
    return true;
}

/// Returns true iff `name` starts with `"zombie-wt-"` and the suffix passes
/// `isSafeIdentifierSegment`. Used by the worktree cleanup sweep.
pub fn isSafeWorktreeDirName(name: []const u8) bool {
    const prefix = "zombie-wt-";
    if (!std.mem.startsWith(u8, name, prefix)) return false;
    return isSafeIdentifierSegment(name[prefix.len..]);
}

test "isSafeIdentifierSegment accepts bounded identifiers" {
    try std.testing.expect(isSafeIdentifierSegment("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(isSafeIdentifierSegment("run-abc.42"));
    try std.testing.expect(!isSafeIdentifierSegment(""));
    try std.testing.expect(!isSafeIdentifierSegment("../ws"));
    try std.testing.expect(!isSafeIdentifierSegment("ws/123"));
    try std.testing.expect(!isSafeIdentifierSegment("ws 123"));
}

test "isSafeWorktreeDirName accepts only controlled prefix and identifiers" {
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-r-abc.1"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-"));
    try std.testing.expect(!isSafeWorktreeDirName("tmp-zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run/123"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run 123"));
}
