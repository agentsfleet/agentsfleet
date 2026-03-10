//! Git operations — bare clone + worktree, commit + push per stage,
//! PR creation via GitHub REST API.

const std = @import("std");
const log = std.log.scoped(.git);

pub const GitError = error{
    CloneFailed,
    FetchFailed,
    WorktreeFailed,
    CommitFailed,
    PushFailed,
    PrFailed,
    InvalidGitHubUrl,
    MissingGitHubPat,
    CommandFailed,
    CommandTimedOut,
    PrRateLimited,
    PrAuthFailed,
    PrInvalidRequest,
    PrServerError,
    InvalidIdentifier,
    InvalidGitRef,
    InvalidRelativePath,
};

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

const CommandResources = struct {
    child: std.process.Child,
    alloc: std.mem.Allocator,
    env: std.process.EnvMap,
    stdout: ?[]u8 = null,
    stderr: ?[]u8 = null,

    fn init(
        alloc: std.mem.Allocator,
        argv: []const []const u8,
        cwd: ?[]const u8,
    ) !CommandResources {
        var child = std.process.Child.init(argv, alloc);
        child.cwd = cwd;
        child.stdout_behavior = .Pipe;
        child.stderr_behavior = .Pipe;
        var env = try sanitizedChildEnv(alloc);
        errdefer env.deinit();
        child.env_map = &env;
        try child.spawn();
        return .{ .child = child, .alloc = alloc, .env = env };
    }

    fn readOutput(self: *CommandResources) !void {
        self.stdout = if (self.child.stdout) |*s|
            try s.readToEndAlloc(self.alloc, 1024 * 1024)
        else
            try self.alloc.dupe(u8, "");
        errdefer if (self.stdout) |b| self.alloc.free(b);

        self.stderr = if (self.child.stderr) |*s|
            try s.readToEndAlloc(self.alloc, 1024 * 1024)
        else
            try self.alloc.dupe(u8, "");
        errdefer if (self.stderr) |b| self.alloc.free(b);
    }

    fn takeStdout(self: *CommandResources) []const u8 {
        const out = self.stdout orelse "";
        self.stdout = null;
        return out;
    }

    fn stderrOrEmpty(self: *const CommandResources) []const u8 {
        return self.stderr orelse "";
    }

    fn deinit(self: *CommandResources) void {
        if (self.stdout) |b| self.alloc.free(b);
        if (self.stderr) |b| self.alloc.free(b);
        self.env.deinit();
    }
};

fn sanitizedChildEnv(alloc: std.mem.Allocator) !std.process.EnvMap {
    var env = std.process.EnvMap.init(alloc);
    errdefer env.deinit();

    try copyEnvIfPresent(alloc, &env, "PATH");
    try copyEnvIfPresent(alloc, &env, "HOME");
    try copyEnvIfPresent(alloc, &env, "TMPDIR");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_FILE");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_DIR");
    return env;
}

fn copyEnvIfPresent(alloc: std.mem.Allocator, env: *std.process.EnvMap, key: []const u8) !void {
    const value = std.process.getEnvVarOwned(alloc, key) catch return;
    defer alloc.free(value);
    try env.put(key, value);
}

fn isSafeIdentifierSegment(value: []const u8) bool {
    if (value.len == 0) return false;
    for (value) |c| {
        if (std.ascii.isAlphanumeric(c)) continue;
        if (c == '-' or c == '_' or c == '.') continue;
        return false;
    }
    return true;
}

fn isSafeGitRef(ref: []const u8) bool {
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

fn isSafeRelativePath(path: []const u8) bool {
    if (path.len == 0) return false;
    if (std.fs.path.isAbsolute(path)) return false;

    var it = std.mem.splitScalar(u8, path, '/');
    while (it.next()) |segment| {
        if (!isSafeIdentifierSegment(segment)) return false;
    }
    return true;
}

fn isSafeWorktreeDirName(name: []const u8) bool {
    const prefix = "zombie-wt-";
    if (!std.mem.startsWith(u8, name, prefix)) return false;
    return isSafeIdentifierSegment(name[prefix.len..]);
}

fn run(
    alloc: std.mem.Allocator,
    argv: []const []const u8,
    cwd: ?[]const u8,
    timeout_ms: u64,
) ![]const u8 {
    var resources = try CommandResources.init(alloc, argv, cwd);
    defer resources.deinit();
    _ = timeout_ms;
    const term = try resources.child.wait();

    try resources.readOutput();
    const stderr = resources.stderrOrEmpty();

    switch (term) {
        .Exited => |code| if (code != 0) {
            log.err("command failed code={d} argv[0]={s} stderr={s}", .{ code, argv[0], stderr });
            return GitError.CommandFailed;
        },
        else => {
            return GitError.CommandFailed;
        },
    }

    return resources.takeStdout();
}

/// Ensure a bare clone of the repo exists at cache_root/<workspace_id>.git.
pub fn ensureBareClone(
    alloc: std.mem.Allocator,
    cache_root: []const u8,
    workspace_id: []const u8,
    repo_url: []const u8,
) ![]const u8 {
    if (!isSafeIdentifierSegment(workspace_id)) return GitError.InvalidIdentifier;
    const bare_path = try std.fmt.allocPrint(alloc, "{s}/{s}.git", .{ cache_root, workspace_id });

    if (std.fs.accessAbsolute(bare_path, .{})) |_| {
        // Already exists — fetch latest
        const out = run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "fetch", "--all", "--prune" },
            bare_path,
            120_000,
        ) catch return GitError.FetchFailed;
        alloc.free(out);
        log.info("git fetch workspace_id={s}", .{workspace_id});
    } else |_| {
        // Clone bare
        try std.fs.makeDirAbsolute(bare_path);
        const argv = &[_][]const u8{
            "git", "-c", "core.hooksPath=/dev/null", "clone", "--bare", repo_url, bare_path,
        };
        const out = run(alloc, argv, null, 120_000) catch return GitError.CloneFailed;
        alloc.free(out);
        log.info("git clone bare workspace_id={s}", .{workspace_id});
    }

    return bare_path;
}

/// Create a worktree for a run on a new feature branch.
/// Branch name: zombie/run-<run_id>.
/// Returns the worktree path (owned by caller).
pub fn createWorktree(
    alloc: std.mem.Allocator,
    bare_path: []const u8,
    run_id: []const u8,
    base_branch: []const u8,
) !WorktreeHandle {
    if (!isSafeIdentifierSegment(run_id)) return GitError.InvalidIdentifier;
    if (!isSafeGitRef(base_branch)) return GitError.InvalidGitRef;

    const branch = try std.fmt.allocPrint(alloc, "zombie/run-{s}", .{run_id});
    defer alloc.free(branch);
    if (!isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const wt_path = try std.fmt.allocPrint(alloc, "/tmp/zombie-wt-{s}", .{run_id});

    // Create worktree with new branch based on base_branch
    const argv = &[_][]const u8{
        "git", "-c", "core.hooksPath=/dev/null", "worktree", "add", "-b", branch, wt_path, base_branch,
    };
    const out = run(alloc, argv, bare_path, 120_000) catch return GitError.WorktreeFailed;
    alloc.free(out);
    log.info("worktree created path={s} branch={s}", .{ wt_path, branch });

    return WorktreeHandle{ .path = wt_path, .alloc = alloc };
}

/// Remove a worktree and prune stale entries.
pub fn removeWorktree(
    alloc: std.mem.Allocator,
    bare_path: []const u8,
    wt_path: []const u8,
) void {
    const out1 = run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "worktree", "remove", "--force", wt_path },
        bare_path,
        30_000,
    ) catch {
        log.warn("worktree remove failed path={s}", .{wt_path});
        return;
    };
    alloc.free(out1);

    const out2 = run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "worktree", "prune" },
        bare_path,
        30_000,
    ) catch return;
    alloc.free(out2);
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
        if (!isSafeWorktreeDirName(entry.name)) continue;

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
        if (!isSafeIdentifierSegment(stem)) continue;

        const bare_path = std.fmt.allocPrint(alloc, "{s}/{s}", .{ cache_root, entry.name }) catch continue;
        defer alloc.free(bare_path);
        const out = run(
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
    if (!isSafeRelativePath(rel_path)) return GitError.InvalidRelativePath;

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
    const add_out = run(
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

    const commit_out = run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", cfg_name, "-c", cfg_email, "commit", "-m", message },
        wt_path,
        30_000,
    ) catch |err| switch (err) {
        GitError.CommandFailed => {
            // Idempotent artifact writes can re-run with identical content.
            const status = run(
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

    log.info("committed file={s} msg={s}", .{ rel_path, message });
}

/// Push the feature branch to origin.
pub fn push(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    branch: []const u8,
    github_token: ?[]const u8,
) !void {
    if (!isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const refspec = try std.fmt.allocPrint(alloc, "HEAD:refs/heads/{s}", .{branch});
    defer alloc.free(refspec);

    const out = if (github_token) |token| blk: {
        const header = try std.fmt.allocPrint(alloc, "http.extraheader=Authorization: Bearer {s}", .{token});
        defer alloc.free(header);
        break :blk run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", header, "push", "origin", refspec },
            wt_path,
            60_000,
        ) catch return GitError.PushFailed;
    } else run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "push", "origin", refspec },
        wt_path,
        60_000,
    ) catch return GitError.PushFailed;

    alloc.free(out);
    log.info("pushed branch={s}", .{branch});
}

pub fn remoteBranchExists(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    branch: []const u8,
    github_token: ?[]const u8,
) !bool {
    if (!isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const remote_ref = try std.fmt.allocPrint(alloc, "refs/heads/{s}", .{branch});
    defer alloc.free(remote_ref);

    const out = if (github_token) |token| blk: {
        const header = try std.fmt.allocPrint(alloc, "http.extraheader=Authorization: Bearer {s}", .{token});
        defer alloc.free(header);
        break :blk run(
            alloc,
            &.{ "git", "-c", "core.hooksPath=/dev/null", "-c", header, "ls-remote", "--heads", "origin", remote_ref },
            wt_path,
            30_000,
        ) catch return GitError.FetchFailed;
    } else run(
        alloc,
        &.{ "git", "-c", "core.hooksPath=/dev/null", "ls-remote", "--heads", "origin", remote_ref },
        wt_path,
        30_000,
    ) catch return GitError.FetchFailed;
    defer alloc.free(out);

    const trimmed = std.mem.trim(u8, out, " \r\n\t");
    return trimmed.len > 0;
}

/// Create a GitHub pull request via REST API.
/// Returns the PR URL (caller owns).
pub fn createPullRequest(
    alloc: std.mem.Allocator,
    repo_url: []const u8,
    branch: []const u8,
    base_branch: []const u8,
    title: []const u8,
    body: []const u8,
    github_token: []const u8,
    error_detail_out: ?*?[]u8,
) ![]const u8 {
    if (error_detail_out) |out| out.* = null;
    if (!isSafeGitRef(branch) or !isSafeGitRef(base_branch)) return GitError.InvalidGitRef;

    // Parse owner/repo from URL
    // e.g. https://github.com/owner/repo.git → owner/repo
    const owner_repo = try parseGitHubOwnerRepo(alloc, repo_url);
    defer alloc.free(owner_repo);

    const api_url = try std.fmt.allocPrint(
        alloc,
        "https://api.github.com/repos/{s}/pulls",
        .{owner_repo},
    );
    defer alloc.free(api_url);

    const payload = try std.json.Stringify.valueAlloc(alloc, .{
        .title = title,
        .head = branch,
        .base = base_branch,
        .body = body,
    }, .{});
    defer alloc.free(payload);

    const auth_header = try std.fmt.allocPrint(alloc, "Authorization: Bearer {s}", .{github_token});
    defer alloc.free(auth_header);

    const result = run(alloc, &.{
        "curl",                                "-sS",
        "--connect-timeout",                   "10",
        "--max-time",                          "30",
        "-i",                                  "-X",
        "POST",                                "-H",
        "Content-Type: application/json",      "-H",
        "Accept: application/vnd.github+json", "-H",
        auth_header,                           "-d",
        payload,                               api_url,
    }, null, 30_000) catch return GitError.PrFailed;
    defer alloc.free(result);

    const parsed_response = splitHttpResponse(result);
    const status = parseHttpStatus(parsed_response.headers) orelse {
        log.err("failed to parse PR creation HTTP status body={s}", .{parsed_response.body});
        return GitError.PrFailed;
    };

    if (status >= 400) {
        if (error_detail_out) |out| {
            out.* = alloc.dupe(u8, result) catch null;
        }
        log.err("PR creation failed status={d} body={s}", .{ status, parsed_response.body });
        return switch (status) {
            429 => GitError.PrRateLimited,
            401, 403 => GitError.PrAuthFailed,
            400, 422 => GitError.PrInvalidRequest,
            500...599 => GitError.PrServerError,
            else => GitError.PrFailed,
        };
    }

    // Parse pr.html_url from response body
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, parsed_response.body, .{});
    defer parsed.deinit();

    if (parsed.value.object.get("html_url")) |url_val| {
        return alloc.dupe(u8, url_val.string);
    }

    log.err("PR creation response missing html_url: {s}", .{result});
    return GitError.PrFailed;
}

/// Find an already-open pull request URL for owner:branch.
/// Returns null when no PR exists.
pub fn findOpenPullRequestByHead(
    alloc: std.mem.Allocator,
    repo_url: []const u8,
    branch: []const u8,
    github_token: []const u8,
    error_detail_out: ?*?[]u8,
) !?[]const u8 {
    if (error_detail_out) |out| out.* = null;
    if (!isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const owner_repo = try parseGitHubOwnerRepo(alloc, repo_url);
    defer alloc.free(owner_repo);

    const slash = std.mem.indexOfScalar(u8, owner_repo, '/') orelse return GitError.InvalidGitHubUrl;
    const owner = owner_repo[0..slash];

    const head_selector = try std.fmt.allocPrint(alloc, "head={s}:{s}", .{ owner, branch });
    defer alloc.free(head_selector);

    const api_url = try std.fmt.allocPrint(
        alloc,
        "https://api.github.com/repos/{s}/pulls",
        .{owner_repo},
    );
    defer alloc.free(api_url);

    const auth_header = try std.fmt.allocPrint(alloc, "Authorization: Bearer {s}", .{github_token});
    defer alloc.free(auth_header);

    const result = run(alloc, &.{
        "curl",              "-sS",
        "--connect-timeout", "10",
        "--max-time",        "30",
        "-i",                "-G",
        "-H",                "Accept: application/vnd.github+json",
        "-H",                auth_header,
        "--data-urlencode",  "state=open",
        "--data-urlencode",  head_selector,
        api_url,
    }, null, 30_000) catch return GitError.PrFailed;
    defer alloc.free(result);

    const parsed_response = splitHttpResponse(result);
    const status = parseHttpStatus(parsed_response.headers) orelse return GitError.PrFailed;
    if (status >= 400) {
        if (error_detail_out) |out| out.* = alloc.dupe(u8, result) catch null;
        return switch (status) {
            429 => GitError.PrRateLimited,
            401, 403 => GitError.PrAuthFailed,
            400, 404, 422 => GitError.PrInvalidRequest,
            500...599 => GitError.PrServerError,
            else => GitError.PrFailed,
        };
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, parsed_response.body, .{});
    defer parsed.deinit();
    if (parsed.value != .array or parsed.value.array.items.len == 0) return null;

    const first = parsed.value.array.items[0];
    if (first != .object) return null;
    const html_url = first.object.get("html_url") orelse return null;
    if (html_url != .string) return null;
    const url_copy = try alloc.dupe(u8, html_url.string);
    return url_copy;
}

const HttpResponseParts = struct {
    headers: []const u8,
    body: []const u8,
};

fn splitHttpResponse(response: []const u8) HttpResponseParts {
    if (std.mem.lastIndexOf(u8, response, "\r\n\r\n")) |sep| {
        return .{
            .headers = response[0..sep],
            .body = response[sep + 4 ..],
        };
    }
    if (std.mem.lastIndexOf(u8, response, "\n\n")) |sep| {
        return .{
            .headers = response[0..sep],
            .body = response[sep + 2 ..],
        };
    }
    return .{
        .headers = "",
        .body = response,
    };
}

fn parseHttpStatus(headers: []const u8) ?u16 {
    const idx = std.mem.lastIndexOf(u8, headers, "HTTP/") orelse return null;
    const line = headers[idx..];
    const line_end = std.mem.indexOfScalar(u8, line, '\n') orelse line.len;
    const status_line = std.mem.trim(u8, line[0..line_end], " \r\n\t");

    const first_space = std.mem.indexOfScalar(u8, status_line, ' ') orelse return null;
    var rest = status_line[first_space + 1 ..];
    rest = std.mem.trimLeft(u8, rest, " ");
    if (rest.len < 3) return null;

    return std.fmt.parseInt(u16, rest[0..3], 10) catch null;
}

test "parseHttpStatus handles standard status lines" {
    const headers =
        "HTTP/1.1 100 Continue\r\n\r\n" ++
        "HTTP/2 429\r\nRetry-After: 7\r\n";
    try std.testing.expectEqual(@as(?u16, 429), parseHttpStatus(headers));
}

test "splitHttpResponse separates headers and body" {
    const raw = "HTTP/2 201\r\nContent-Type: application/json\r\n\r\n{\"html_url\":\"https://x\"}";
    const parts = splitHttpResponse(raw);
    try std.testing.expect(std.mem.containsAtLeast(u8, parts.headers, 1, "HTTP/2 201"));
    try std.testing.expect(std.mem.eql(u8, parts.body, "{\"html_url\":\"https://x\"}"));
}

test "integration: sanitizedChildEnv keeps minimal allowlist" {
    var env = try sanitizedChildEnv(std.testing.allocator);
    defer env.deinit();

    if (std.process.getEnvVarOwned(std.testing.allocator, "PATH")) |value| {
        defer std.testing.allocator.free(value);
        try std.testing.expect(env.get("PATH") != null);
    } else |_| {}
    try std.testing.expect(env.get("GIT_DIR") == null);
    try std.testing.expect(env.get("GIT_WORK_TREE") == null);
    try std.testing.expect(env.get("GITHUB_TOKEN") == null);
}

test "isSafeIdentifierSegment accepts bounded identifiers" {
    try std.testing.expect(isSafeIdentifierSegment("ws_123"));
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

test "integration: remoteBranchExists rejects invalid refs" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        remoteBranchExists(std.testing.allocator, "/tmp", "../main", null),
    );
}

fn parseGitHubOwnerRepo(alloc: std.mem.Allocator, repo_url: []const u8) ![]const u8 {
    // Handle both https://github.com/owner/repo.git and git@github.com:owner/repo.git
    const github_prefix_https = "https://github.com/";
    const github_prefix_ssh = "git@github.com:";

    var rest: []const u8 = undefined;
    if (std.mem.startsWith(u8, repo_url, github_prefix_https)) {
        rest = repo_url[github_prefix_https.len..];
    } else if (std.mem.startsWith(u8, repo_url, github_prefix_ssh)) {
        rest = repo_url[github_prefix_ssh.len..];
    } else {
        return GitError.InvalidGitHubUrl;
    }

    // Strip .git suffix if present
    const owner_repo = if (std.mem.endsWith(u8, rest, ".git"))
        rest[0 .. rest.len - 4]
    else
        rest;

    const slash = std.mem.indexOfScalar(u8, owner_repo, '/') orelse return GitError.InvalidGitHubUrl;
    if (std.mem.lastIndexOfScalar(u8, owner_repo, '/') != slash) return GitError.InvalidGitHubUrl;

    const owner = owner_repo[0..slash];
    const repo = owner_repo[slash + 1 ..];
    if (!isSafeIdentifierSegment(owner) or !isSafeIdentifierSegment(repo)) return GitError.InvalidGitHubUrl;

    return alloc.dupe(u8, owner_repo);
}

test "isSafeRelativePath rejects traversal-like relative paths" {
    try std.testing.expect(isSafeRelativePath("docs/runs/a.md"));
    try std.testing.expect(!isSafeRelativePath(""));
    try std.testing.expect(!isSafeRelativePath("../docs/runs/a.md"));
    try std.testing.expect(!isSafeRelativePath("/tmp/a.md"));
    try std.testing.expect(!isSafeRelativePath("docs//a.md"));
}

test "parseGitHubOwnerRepo rejects malformed owner/repo path" {
    try std.testing.expectError(
        GitError.InvalidGitHubUrl,
        parseGitHubOwnerRepo(std.testing.allocator, "https://github.com/owner/repo/extra.git"),
    );
    try std.testing.expectError(
        GitError.InvalidGitHubUrl,
        parseGitHubOwnerRepo(std.testing.allocator, "https://github.com/owner/../repo.git"),
    );
}

test "integration: findOpenPullRequestByHead rejects invalid branch ref" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        findOpenPullRequestByHead(std.testing.allocator, "https://github.com/org/repo.git", "../bad", "token", null),
    );
}

test "isSafeWorktreeDirName accepts only controlled prefix and identifiers" {
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-run_123"));
    try std.testing.expect(isSafeWorktreeDirName("zombie-wt-r-abc.1"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-"));
    try std.testing.expect(!isSafeWorktreeDirName("tmp-zombie-wt-run_123"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run/123"));
    try std.testing.expect(!isSafeWorktreeDirName("zombie-wt-run 123"));
}

test "integration: cleanupRuntimeArtifacts removes stale worktrees in root" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.makePath("cache");
    try tmp.dir.makePath("wt/zombie-wt-run_1");
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
        tmp.dir.access("wt/zombie-wt-run_1", .{}),
    );
    try tmp.dir.access("wt/not-zombie", .{});
}
