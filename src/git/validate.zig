//! Input validation helpers for git operations.
//!
//! All functions are pure (no allocations, no I/O).  They are called by
//! repo.zig, pr.zig and ops.zig before any child-process is spawned so that
//! callers get a typed error before touching the filesystem or network.
//!
//! Ownership: no heap allocations — all parameters are borrowed slices.

const std = @import("std");

/// Returns true iff `value` contains only alphanumerics, `-`, `_`, and `.`,
/// and is non-empty.  Used as the building-block for workspace-id and run-id
/// validation.
pub fn isSafeIdentifierSegment(value: []const u8) bool {
    if (value.len == 0) return false;
    for (value) |c| {
        if (std.ascii.isAlphanumeric(c)) continue;
        if (c == '-' or c == '_' or c == '.') continue;
        return false;
    }
    return true;
}

/// Returns true iff `ref` is a syntactically safe git ref name.
/// Rejects empty strings, leading/trailing `/` or `.`, `..`, `//`,
/// `.lock` suffix, and chars forbidden by `git check-ref-format`.
pub fn isSafeGitRef(ref: []const u8) bool {
    if (ref.len == 0) return false;
    if (ref[0] == '/' or ref[0] == '.' or ref[0] == '-') return false;
    if (ref[ref.len - 1] == '/' or ref[ref.len - 1] == '.') return false;
    if (std.mem.endsWith(u8, ref, ".lock")) return false;
    if (std.mem.indexOf(u8, ref, "..") != null) return false;
    if (std.mem.indexOf(u8, ref, "//") != null) return false;
    for (ref) |c| {
        if (c <= 0x20 or c == 0x7f) return false;
        if (c == '~' or c == '^' or c == ':' or c == '?' or c == '*' or c == '[' or c == '\\') return false;
    }
    return true;
}

/// Returns true iff `path` is a relative path whose every `/`-separated
/// segment passes `isSafeIdentifierSegment`.
pub fn isSafeRelativePath(path: []const u8) bool {
    if (path.len == 0) return false;
    if (std.fs.path.isAbsolute(path)) return false;

    var it = std.mem.splitScalar(u8, path, '/');
    while (it.next()) |segment| {
        if (!isSafeIdentifierSegment(segment)) return false;
    }
    return true;
}

/// Returns true iff `name` starts with `"zombie-wt-"` and the suffix passes
/// `isSafeIdentifierSegment`.  Used by the worktree cleanup sweep.
pub fn isSafeWorktreeDirName(name: []const u8) bool {
    const prefix = "zombie-wt-";
    if (!std.mem.startsWith(u8, name, prefix)) return false;
    return isSafeIdentifierSegment(name[prefix.len..]);
}

// ---------------------------------------------------------------------------
// Tests (moved from ops.zig)
// ---------------------------------------------------------------------------

test "isSafeIdentifierSegment accepts bounded identifiers" {
    try std.testing.expect(isSafeIdentifierSegment("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(isSafeIdentifierSegment("run-abc.42"));
    try std.testing.expect(!isSafeIdentifierSegment(""));
    try std.testing.expect(!isSafeIdentifierSegment("../ws"));
    try std.testing.expect(!isSafeIdentifierSegment("ws/123"));
    try std.testing.expect(!isSafeIdentifierSegment("ws 123"));
}

test "isSafeGitRef blocks traversal-like and invalid ref chars" {
    try std.testing.expect(isSafeGitRef("main"));
    try std.testing.expect(isSafeGitRef("zombie/run-r_abc123"));
    try std.testing.expect(!isSafeGitRef(".."));
    try std.testing.expect(!isSafeGitRef("../main"));
    try std.testing.expect(!isSafeGitRef("heads//main"));
    try std.testing.expect(!isSafeGitRef("main.lock"));
    try std.testing.expect(!isSafeGitRef("feature bad"));
}

test "isSafeRelativePath rejects traversal-like relative paths" {
    try std.testing.expect(isSafeRelativePath("docs/runs/a.md"));
    try std.testing.expect(!isSafeRelativePath(""));
    try std.testing.expect(!isSafeRelativePath("../docs/runs/a.md"));
    try std.testing.expect(!isSafeRelativePath("/tmp/a.md"));
    try std.testing.expect(!isSafeRelativePath("docs//a.md"));
}

test "isSafeWorktreeDirName accepts only controlled prefix and identifiers" {
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-r-abc.1"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-"));
    try std.testing.expect(!isSafeWorktreeDirName("tmp-zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run/123"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run 123"));
}

// New T2 tests per spec §4.0
test "isSafeGitRef with single-char valid ref" {
    try std.testing.expect(isSafeGitRef("a"));
    try std.testing.expect(isSafeGitRef("Z"));
    try std.testing.expect(isSafeGitRef("1"));
}

test "isSafeRelativePath with single segment (no slash)" {
    try std.testing.expect(isSafeRelativePath("README.md"));
    try std.testing.expect(isSafeRelativePath("file-name_01"));
    try std.testing.expect(!isSafeRelativePath("bad file"));
}
