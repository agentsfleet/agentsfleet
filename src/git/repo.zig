//! Runtime artifact cleanup for git worktrees and bare-repo prunes.
//!
//! Sole live entry point: `cleanupRuntimeArtifacts(alloc, cache_root, worktree_root)`,
//! invoked by `cmd/preflight.zig` to sweep stale state from prior runs.

const std = @import("std");
const validate = @import("validate.zig");
const command = @import("command.zig");

pub const RuntimeCleanupStats = struct {
    removed_worktrees: u32 = 0,
    failed_worktree_removals: u32 = 0,
    pruned_bare_repos: u32 = 0,
    failed_bare_prunes: u32 = 0,
};

pub fn cleanupRuntimeArtifacts(
    alloc: std.mem.Allocator,
    cache_root: []const u8,
    worktree_root: []const u8,
) RuntimeCleanupStats {
    var stats = RuntimeCleanupStats{};

    cleanupWorktrees(alloc, worktree_root, &stats);
    cleanupBareRepoPrunes(alloc, cache_root, &stats);

    return stats;
}

fn cleanupWorktrees(
    alloc: std.mem.Allocator,
    worktree_root: []const u8,
    stats: *RuntimeCleanupStats,
) void {
    var dir = std.fs.openDirAbsolute(worktree_root, .{ .iterate = true }) catch return;
    defer dir.close();

    var it = dir.iterate();
    while (it.next() catch null) |entry| {
        if (entry.kind != .directory) continue;
        if (!validate.isSafeWorktreeDirName(entry.name)) continue;

        const abs = std.fmt.allocPrint(alloc, "{s}/{s}", .{ worktree_root, entry.name }) catch continue;
        defer alloc.free(abs);
        std.fs.deleteTreeAbsolute(abs) catch {
            stats.failed_worktree_removals += 1;
            continue;
        };
        stats.removed_worktrees += 1;
    }
}

fn cleanupBareRepoPrunes(
    alloc: std.mem.Allocator,
    cache_root: []const u8,
    stats: *RuntimeCleanupStats,
) void {
    var dir = std.fs.openDirAbsolute(cache_root, .{ .iterate = true }) catch return;
    defer dir.close();

    var it = dir.iterate();
    while (it.next() catch null) |entry| {
        if (entry.kind != .directory) continue;
        if (!std.mem.endsWith(u8, entry.name, ".git")) continue;

        const stem = entry.name[0 .. entry.name.len - 4];
        if (!validate.isSafeIdentifierSegment(stem)) continue;

        const bare_path = std.fmt.allocPrint(alloc, "{s}/{s}", .{ cache_root, entry.name }) catch continue;
        defer alloc.free(bare_path);
        const out = command.run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "worktree", "prune" },
            bare_path,
            30_000,
        ) catch {
            stats.failed_bare_prunes += 1;
            continue;
        };
        alloc.free(out);
        stats.pruned_bare_repos += 1;
    }
}

test "integration: cleanupRuntimeArtifacts removes stale worktrees in root" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.makePath("cache");
    try tmp.dir.makePath("wt/zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    try tmp.dir.makePath("wt/not-zombie");

    const cache_root = try tmp.dir.realpathAlloc(std.testing.allocator, "cache");
    defer std.testing.allocator.free(cache_root);
    const wt_root = try tmp.dir.realpathAlloc(std.testing.allocator, "wt");
    defer std.testing.allocator.free(wt_root);

    const stats = cleanupRuntimeArtifacts(std.testing.allocator, cache_root, wt_root);
    try std.testing.expectEqual(@as(u32, 1), stats.removed_worktrees);
    try std.testing.expectEqual(@as(u32, 0), stats.failed_worktree_removals);

    try std.testing.expectError(
        error.FileNotFound,
        tmp.dir.access("wt/zombie-wt-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{}),
    );
    try tmp.dir.access("wt/not-zombie", .{});
}
