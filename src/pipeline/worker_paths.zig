const std = @import("std");
const worker_runtime = @import("worker_runtime.zig");

pub fn isWithinPath(base: []const u8, candidate: []const u8) bool {
    if (!std.mem.startsWith(u8, candidate, base)) return false;
    if (candidate.len == base.len) return true;
    return candidate[base.len] == std.fs.path.sep;
}

pub fn isSafeRelativeSpecPath(relative_spec_path: []const u8) bool {
    if (relative_spec_path.len == 0) return false;
    if (std.fs.path.isAbsolute(relative_spec_path)) return false;

    var it = std.mem.splitScalar(u8, relative_spec_path, '/');
    while (it.next()) |segment| {
        if (segment.len == 0) return false;
        if (std.mem.eql(u8, segment, ".") or std.mem.eql(u8, segment, "..")) return false;
    }
    return true;
}

pub fn resolveSpecPath(
    alloc: std.mem.Allocator,
    worktree_path: []const u8,
    relative_spec_path: []const u8,
) ![]const u8 {
    if (!isSafeRelativeSpecPath(relative_spec_path)) {
        return worker_runtime.WorkerError.PathTraversal;
    }

    const joined = try std.fs.path.join(alloc, &.{ worktree_path, relative_spec_path });
    defer alloc.free(joined);

    const canonical_spec = try std.fs.realpathAlloc(alloc, joined);
    errdefer alloc.free(canonical_spec);

    const canonical_worktree = try std.fs.realpathAlloc(alloc, worktree_path);
    defer alloc.free(canonical_worktree);

    if (!isWithinPath(canonical_worktree, canonical_spec)) {
        return worker_runtime.WorkerError.PathTraversal;
    }

    return canonical_spec;
}

test "isWithinPath respects directory boundaries" {
    try std.testing.expect(isWithinPath("/tmp/wt", "/tmp/wt/docs/spec.md"));
    try std.testing.expect(isWithinPath("/tmp/wt", "/tmp/wt"));
    try std.testing.expect(!isWithinPath("/tmp/wt", "/tmp/wt-other/spec.md"));
    try std.testing.expect(!isWithinPath("/tmp/wt", "/tmp/other/spec.md"));
}

test "isSafeRelativeSpecPath rejects absolute and traversal paths" {
    try std.testing.expect(isSafeRelativeSpecPath("docs/spec.md"));
    try std.testing.expect(isSafeRelativeSpecPath("nested/path/spec.md"));
    try std.testing.expect(!isSafeRelativeSpecPath(""));
    try std.testing.expect(!isSafeRelativeSpecPath("/etc/passwd"));
    try std.testing.expect(!isSafeRelativeSpecPath("../spec.md"));
    try std.testing.expect(!isSafeRelativeSpecPath("docs/../spec.md"));
    try std.testing.expect(!isSafeRelativeSpecPath("docs//spec.md"));
}

test "integration: relative path checker avoids false positives on dotted names" {
    try std.testing.expect(isSafeRelativeSpecPath("docs/.../spec.md"));
    try std.testing.expect(isSafeRelativeSpecPath("docs/.well-known/spec.md"));
    try std.testing.expect(!isSafeRelativeSpecPath("docs/./spec.md"));
}

test "integration: resolveSpecPath enforces relative path boundary" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.makePath("repo/docs");
    {
        var f = try tmp.dir.createFile("repo/docs/spec.md", .{});
        defer f.close();
        try f.writeAll("# spec");
    }

    const worktree_path = try tmp.dir.realpathAlloc(std.testing.allocator, "repo");
    defer std.testing.allocator.free(worktree_path);

    const resolved = try resolveSpecPath(std.testing.allocator, worktree_path, "docs/spec.md");
    defer std.testing.allocator.free(resolved);
    try std.testing.expect(std.mem.startsWith(u8, resolved, worktree_path));
    try std.testing.expectError(
        worker_runtime.WorkerError.PathTraversal,
        resolveSpecPath(std.testing.allocator, worktree_path, "../etc/passwd"),
    );
}
