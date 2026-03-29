//! Git repository management — bare clone, worktree lifecycle, and runtime
//! artifact cleanup.
//!
//! Exports:
//!   - `WorktreeHandle`         — RAII path handle returned by `createWorktree`.
//!   - `RuntimeCleanupStats`    — summary of a `cleanupRuntimeArtifacts` pass.
//!   - `ensureBareClone`        — create or refresh a bare-clone cache.
//!   - `createWorktree`         — add a new git worktree on a fresh branch.
//!   - `removeWorktree`         — prune a specific worktree by path.
//!   - `cleanupRuntimeArtifacts`— sweep stale worktrees and bare-repo prunes.
//!
//! Ownership:
//!   - `WorktreeHandle.path` is allocated by the caller's allocator.  Call
//!     `WorktreeHandle.deinit()` to free it.
//!   - `ensureBareClone` returns a caller-owned `[]u8`; caller must free.
//!   - All other functions either take ownership of slices they receive or
//!     free internally before returning.

const std = @import("std");
const GitError = @import("errors.zig").GitError;
const validate = @import("validate.zig");
const command = @import("command.zig");
const log = std.log.scoped(.git);

pub const WorktreeHandle = struct {
    path: []const u8, // owned
    alloc: std.mem.Allocator,

    pub fn deinit(self: *WorktreeHandle) void {
        self.alloc.free(self.path);
    }
};

pub const RuntimeCleanupStats = struct {
    removed_worktrees: u32 = 0,
    failed_worktree_removals: u32 = 0,
    pruned_bare_repos: u32 = 0,
    failed_bare_prunes: u32 = 0,
};

/// Ensure a bare clone of the repo exists at `cache_root/<workspace_id>.git`.
/// Returns the bare path (caller-owned).
pub fn ensureBareClone(
    alloc: std.mem.Allocator,
    cache_root: []const u8,
    workspace_id: []const u8,
    repo_url: []const u8,
) ![]const u8 {
    if (!validate.isSafeIdentifierSegment(workspace_id)) return GitError.InvalidIdentifier;
    const bare_path = try std.fmt.allocPrint(alloc, "{s}/{s}.git", .{ cache_root, workspace_id });

    if (std.fs.accessAbsolute(bare_path, .{})) |_| {
        // Already exists — fetch latest
        const out = command.run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "fetch", "--all", "--prune" },
            bare_path,
            120_000,
        ) catch return GitError.FetchFailed;
        alloc.free(out);
        log.info("git.fetch workspace_id={s}", .{workspace_id});
    } else |_| {
        // Clone bare
        try std.fs.makeDirAbsolute(bare_path);
        const argv = &[_][]const u8{
            "git", "-c", "core.hooksPath=/dev/null", "clone", "--bare", repo_url, bare_path,
        };
        const out = command.run(alloc, argv, null, 120_000) catch return GitError.CloneFailed;
        alloc.free(out);
        log.info("git.clone_bare workspace_id={s}", .{workspace_id});
    }

    return bare_path;
}

/// Create a worktree for a run on a new feature branch.
/// Branch name: `zombie/run-<run_id>`.
/// Returns a `WorktreeHandle`; caller must call `deinit`.
pub fn createWorktree(
    alloc: std.mem.Allocator,
    bare_path: []const u8,
    run_id: []const u8,
    base_branch: []const u8,
) !WorktreeHandle {
    if (!validate.isSafeIdentifierSegment(run_id)) return GitError.InvalidIdentifier;
    if (!validate.isSafeGitRef(base_branch)) return GitError.InvalidGitRef;

    const branch = try std.fmt.allocPrint(alloc, "zombie/run-{s}", .{run_id});
    defer alloc.free(branch);
    if (!validate.isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const wt_path = try std.fmt.allocPrint(alloc, "/tmp/zombie-wt-{s}", .{run_id});

    // Create worktree with new branch based on base_branch
    const argv = &[_][]const u8{
        "git", "-c", "core.hooksPath=/dev/null", "worktree", "add", "-b", branch, wt_path, base_branch,
    };
    const out = command.run(alloc, argv, bare_path, 120_000) catch return GitError.WorktreeFailed;
    alloc.free(out);
    log.info("git.worktree_created path={s} branch={s}", .{ wt_path, branch });

    return WorktreeHandle{ .path = wt_path, .alloc = alloc };
}

/// Remove a worktree and prune stale entries.
pub fn removeWorktree(
    alloc: std.mem.Allocator,
    bare_path: []const u8,
    wt_path: []const u8,
) void {
    const out1 = command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "worktree", "remove", "--force", wt_path },
        bare_path,
        30_000,
    ) catch {
        log.warn("git.worktree_remove_fail path={s}", .{wt_path});
        return;
    };
    alloc.free(out1);

    const out2 = command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "worktree", "prune" },
        bare_path,
        30_000,
    ) catch return;
    alloc.free(out2);
}

/// Get the HEAD commit SHA of a worktree.
pub fn getHeadSha(alloc: std.mem.Allocator, wt_path: []const u8) ![]const u8 {
    const out = try command.run(alloc, &.{ "git", "rev-parse", "HEAD" }, wt_path, 10_000);
    // Trim trailing newline.
    const trimmed = std.mem.trimRight(u8, out, "\n\r ");
    if (trimmed.len == out.len) return out;
    const result = try alloc.dupe(u8, trimmed);
    alloc.free(out);
    return result;
}

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

/// Write a file in the worktree and commit it.
pub fn commitFile(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    rel_path: []const u8,
    content: []const u8,
    message: []const u8,
    author_name: []const u8,
    author_email: []const u8,
) !void {
    if (!validate.isSafeRelativePath(rel_path)) return GitError.InvalidRelativePath;

    // Write file
    const abs_path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ wt_path, rel_path });
    defer alloc.free(abs_path);

    // Ensure parent directory exists
    if (std.fs.path.dirname(abs_path)) |dir| {
        std.fs.makeDirAbsolute(dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
    }

    const file = try std.fs.createFileAbsolute(abs_path, .{});
    defer file.close();
    try file.writeAll(content);

    // git add + commit
    const add_out = command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "add", rel_path },
        wt_path,
        30_000,
    ) catch return GitError.CommitFailed;
    alloc.free(add_out);

    const cfg_name = try std.fmt.allocPrint(alloc, "user.name={s}", .{author_name});
    defer alloc.free(cfg_name);
    const cfg_email = try std.fmt.allocPrint(alloc, "user.email={s}", .{author_email});
    defer alloc.free(cfg_email);

    const commit_out = command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", cfg_name, "-c", cfg_email, "commit", "-m", message },
        wt_path,
        30_000,
    ) catch |err| switch (err) {
        GitError.CommandFailed => {
            // Idempotent artifact writes can re-run with identical content.
            const status = command.run(
                alloc,
                &.{ "git", "-c", "core.hooksPath=/dev/null", "status", "--porcelain", "--", rel_path },
                wt_path,
                10_000,
            ) catch return GitError.CommitFailed;
            defer alloc.free(status);
            if (status.len == 0) return;
            return GitError.CommitFailed;
        },
        else => return GitError.CommitFailed,
    };
    alloc.free(commit_out);

    log.info("git.committed file={s} msg={s}", .{ rel_path, message });
}

/// Push the feature branch to origin.
pub fn push(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    branch: []const u8,
    github_token: ?[]const u8,
) !void {
    if (!validate.isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const refspec = try std.fmt.allocPrint(alloc, "HEAD:refs/heads/{s}", .{branch});
    defer alloc.free(refspec);

    const out = if (github_token) |token| blk: {
        const header = try std.fmt.allocPrint(alloc, "http.extraheader=Authorization: Bearer {s}", .{token});
        defer alloc.free(header);
        break :blk command.run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", header, "push", "origin", refspec },
            wt_path,
            60_000,
        ) catch return GitError.PushFailed;
    } else command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "push", "origin", refspec },
        wt_path,
        60_000,
    ) catch return GitError.PushFailed;

    alloc.free(out);
    log.info("git.pushed branch={s}", .{branch});
}

pub fn remoteBranchExists(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    branch: []const u8,
    github_token: ?[]const u8,
) !bool {
    if (!validate.isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const remote_ref = try std.fmt.allocPrint(alloc, "refs/heads/{s}", .{branch});
    defer alloc.free(remote_ref);

    const out = if (github_token) |token| blk: {
        const header = try std.fmt.allocPrint(alloc, "http.extraheader=Authorization: Bearer {s}", .{token});
        defer alloc.free(header);
        break :blk command.run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", header, "ls-remote", "--heads", "origin", remote_ref },
            wt_path,
            30_000,
        ) catch return GitError.FetchFailed;
    } else command.run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "ls-remote", "--heads", "origin", remote_ref },
        wt_path,
        30_000,
    ) catch return GitError.FetchFailed;
    defer alloc.free(out);

    const trimmed = std.mem.trim(u8, out, " \r\n\t");
    return trimmed.len > 0;
}

// ---------------------------------------------------------------------------
// Tests (moved from ops.zig)
// ---------------------------------------------------------------------------

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

test "integration: remoteBranchExists rejects invalid refs" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        remoteBranchExists(std.testing.allocator, "/tmp", "../main", null),
    );
}

// New T3 tests per spec §4.0
test "commitFile with invalid relative path returns InvalidRelativePath" {
    try std.testing.expectError(
        GitError.InvalidRelativePath,
        commitFile(
            std.testing.allocator,
            "/tmp",
            "../etc/passwd",
            "content",
            "msg",
            "Author",
            "author@example.com",
        ),
    );
}

test "push with invalid branch ref returns InvalidGitRef" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        push(std.testing.allocator, "/tmp", "../bad-branch", null),
    );
}
