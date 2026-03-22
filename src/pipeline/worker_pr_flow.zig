const std = @import("std");
const git = @import("../git/ops.zig");
const pg = @import("pg");
const github_auth = @import("../auth/github.zig");
const reliable = @import("../reliability/reliable_call.zig");
const state = @import("../state/machine.zig");
const secrets = @import("../secrets/crypto.zig");
const worker_runtime = @import("worker_runtime.zig");
const side_effect_keys = @import("worker_side_effect_keys.zig");
const worker_rate_limiter = @import("worker_rate_limiter.zig");

const log = std.log.scoped(.agents);

pub const PushFailureAction = enum {
    lookup_remote_branch,
};

pub const PrCreateFailureAction = enum {
    retry,
    lookup_existing_pr,
    fail,
};

pub fn onPushFailure() PushFailureAction {
    return .lookup_remote_branch;
}

pub fn onPrCreateFailure(err: anyerror, detail: ?[]const u8) PrCreateFailureAction {
    _ = detail;
    return switch (err) {
        git.GitError.PrRateLimited, git.GitError.PrServerError => .retry,
        git.GitError.PrInvalidRequest, git.GitError.PrFailed => .lookup_existing_pr,
        else => .fail,
    };
}

pub fn indicatesClosedOrDeleted(detail: ?[]const u8) bool {
    const d = detail orelse return false;
    return containsIgnoreCase(d, "closed") or
        containsIgnoreCase(d, "deleted") or
        containsIgnoreCase(d, "no commits between") or
        containsIgnoreCase(d, "head sha can't be blank") or
        containsIgnoreCase(d, "head ref must be a branch");
}

pub fn indicatesNothingToCommit(detail: ?[]const u8) bool {
    const d = detail orelse return false;
    return containsIgnoreCase(d, "nothing to commit") or
        containsIgnoreCase(d, "working tree clean");
}

pub fn indicatesDirtyWorktree(detail: ?[]const u8) bool {
    const d = detail orelse return false;
    return containsIgnoreCase(d, "changes not staged") or
        containsIgnoreCase(d, "unmerged paths") or
        containsIgnoreCase(d, "please commit your changes");
}

pub fn indicatesPushRejected(detail: ?[]const u8) bool {
    const d = detail orelse return false;
    return containsIgnoreCase(d, "non-fast-forward") or
        containsIgnoreCase(d, "[rejected]") or
        containsIgnoreCase(d, "failed to push some refs");
}

pub fn indicatesPrConflict(detail: ?[]const u8) bool {
    const d = detail orelse return false;
    return containsIgnoreCase(d, "merge conflict") or
        containsIgnoreCase(d, "has conflicts") or
        containsIgnoreCase(d, "not mergeable");
}

pub const PrRunContext = struct {
    run_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    spec_id: []const u8,
    repo_url: []const u8,
    default_branch: []const u8,
    branch: []const u8,
    final_stage_output: []const u8,
};

const TokenRetryCtx = struct {
    cache: *github_auth.TokenCache,
    alloc: std.mem.Allocator,
    installation_id: []const u8,
    last_error_detail: ?[]u8 = null,
};

fn opGetInstallationToken(ctx: *TokenRetryCtx, _: u32) ![]u8 {
    if (ctx.last_error_detail) |d| ctx.alloc.free(d);
    ctx.last_error_detail = null;
    return ctx.cache.getInstallationTokenWithDetail(ctx.alloc, ctx.installation_id, &ctx.last_error_detail);
}

fn tokenDetail(ctx: *TokenRetryCtx, _: anyerror) ?[]const u8 {
    return ctx.last_error_detail;
}

const PushRetryCtx = struct {
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    branch: []const u8,
    github_token: []const u8,
};

fn opPushBranch(ctx: PushRetryCtx, _: u32) !void {
    return git.push(ctx.alloc, ctx.wt_path, ctx.branch, ctx.github_token);
}

const PrRetryCtx = struct {
    alloc: std.mem.Allocator,
    repo_url: []const u8,
    branch: []const u8,
    base_branch: []const u8,
    title: []const u8,
    body: []const u8,
    github_token: []const u8,
    last_error_detail: ?[]u8 = null,
};

fn opCreatePr(ctx: *PrRetryCtx, _: u32) ![]u8 {
    ctx.last_error_detail = null;
    const created_pr = try git.createPullRequest(
        ctx.alloc,
        ctx.repo_url,
        ctx.branch,
        ctx.base_branch,
        ctx.title,
        ctx.body,
        ctx.github_token,
        &ctx.last_error_detail,
    );
    defer ctx.alloc.free(created_pr);
    return try ctx.alloc.dupe(u8, created_pr);
}

fn prDetail(ctx: *PrRetryCtx, _: anyerror) ?[]const u8 {
    return ctx.last_error_detail;
}

fn loadInstallationId(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
) ![]u8 {
    return secrets.load(alloc, conn, workspace_id, "github_app_installation_id") catch worker_runtime.WorkerError.MissingGitHubInstallation;
}

fn getExistingPrUrl(alloc: std.mem.Allocator, conn: *pg.Conn, run_id: []const u8) !?[]u8 {
    var result = try conn.query("SELECT pr_url FROM runs WHERE run_id = $1", .{run_id});
    defer result.deinit();

    const row = try result.next() orelse return null;
    const value = try row.get(?[]u8, 0);
    const copy = if (value) |pr| try alloc.dupe(u8, pr) else null;
    try result.drain();
    return copy;
}

fn tryRecoverPrUrl(
    run_alloc: std.mem.Allocator,
    conn: *pg.Conn,
    token_cache: *github_auth.TokenCache,
    tenant_limiter: *worker_rate_limiter.TenantRateLimiter,
    cancel_flag: *const std.atomic.Value(bool),
    deadline_ms: i64,
    ctx: PrRunContext,
) !?[]u8 {
    try worker_runtime.ensureRunActive(cancel_flag, deadline_ms);
    const installation_id = loadInstallationId(run_alloc, conn, ctx.workspace_id) catch return null;
    defer run_alloc.free(installation_id);

    try tenant_limiter.acquireCancelable(ctx.tenant_id, "github_installation_token", 1.0, cancel_flag, deadline_ms);
    var token_retry_ctx = TokenRetryCtx{
        .cache = token_cache,
        .alloc = run_alloc,
        .installation_id = installation_id,
    };
    defer if (token_retry_ctx.last_error_detail) |d| run_alloc.free(d);
    const github_token = try reliable.callWithDetail(
        []u8,
        &token_retry_ctx,
        opGetInstallationToken,
        tokenDetail,
        worker_runtime.retryOptionsForRun(@constCast(cancel_flag), deadline_ms, 2, 500, 5_000, "github_installation_token"),
    );
    defer run_alloc.free(github_token);

    try tenant_limiter.acquireCancelable(ctx.tenant_id, "github_pr_lookup", 1.0, cancel_flag, deadline_ms);
    const existing = try git.findOpenPullRequestByHead(run_alloc, ctx.repo_url, ctx.branch, github_token, null);
    if (existing) |pr_url| {
        var side_effect_key_buf: [192]u8 = undefined;
        const side_effect_key = try side_effect_keys.sideEffectKeyPrCreate(run_alloc, &side_effect_key_buf, ctx.branch);
        defer side_effect_key.deinit(run_alloc);
        try state.markSideEffectDone(conn, ctx.run_id, side_effect_key.value, pr_url);
        return try run_alloc.dupe(u8, pr_url);
    }
    return null;
}

pub fn ensurePrForRun(
    run_alloc: std.mem.Allocator,
    conn: *pg.Conn,
    token_cache: *github_auth.TokenCache,
    tenant_limiter: *worker_rate_limiter.TenantRateLimiter,
    cancel_flag: *const std.atomic.Value(bool),
    deadline_ms: i64,
    wt_path: []const u8,
    ctx: PrRunContext,
) ![]u8 {
    var pr_url = try getExistingPrUrl(run_alloc, conn, ctx.run_id);
    if (pr_url == null) {
        try worker_runtime.ensureRunActive(cancel_flag, deadline_ms);
        const installation_id = try loadInstallationId(run_alloc, conn, ctx.workspace_id);
        defer run_alloc.free(installation_id);

        try tenant_limiter.acquireCancelable(ctx.tenant_id, "github_installation_token", 1.0, cancel_flag, deadline_ms);
        var token_retry_ctx = TokenRetryCtx{
            .cache = token_cache,
            .alloc = run_alloc,
            .installation_id = installation_id,
        };
        defer if (token_retry_ctx.last_error_detail) |d| run_alloc.free(d);
        const github_token = try reliable.callWithDetail(
            []u8,
            &token_retry_ctx,
            opGetInstallationToken,
            tokenDetail,
            worker_runtime.retryOptionsForRun(@constCast(cancel_flag), deadline_ms, 2, 500, 5_000, "github_installation_token"),
        );
        defer run_alloc.free(github_token);

        var push_side_effect_key_buf: [192]u8 = undefined;
        const push_side_effect_key = try side_effect_keys.sideEffectKeyPush(run_alloc, &push_side_effect_key_buf, ctx.branch);
        defer push_side_effect_key.deinit(run_alloc);
        const push_claimed = try state.claimSideEffect(conn, ctx.run_id, push_side_effect_key.value, ctx.branch);
        if (push_claimed) {
            try worker_runtime.ensureRunActive(cancel_flag, deadline_ms);
            try tenant_limiter.acquireCancelable(ctx.tenant_id, "github_push", 1.0, cancel_flag, deadline_ms);
            reliable.call(void, PushRetryCtx{
                .alloc = run_alloc,
                .wt_path = wt_path,
                .branch = ctx.branch,
                .github_token = github_token,
            }, opPushBranch, worker_runtime.retryOptionsForRun(@constCast(cancel_flag), deadline_ms, 2, 1_000, 10_000, "github_push")) catch |push_err| {
                switch (onPushFailure()) {
                    .lookup_remote_branch => {
                        const already_pushed = try git.remoteBranchExists(run_alloc, wt_path, ctx.branch, github_token);
                        if (!already_pushed) return push_err;
                    },
                }
            };
            try state.markSideEffectDone(conn, ctx.run_id, push_side_effect_key.value, ctx.branch);
        } else {
            const already_pushed = try git.remoteBranchExists(run_alloc, wt_path, ctx.branch, github_token);
            if (!already_pushed) return worker_runtime.WorkerError.SideEffectClaimedNoResult;
            try state.markSideEffectDone(conn, ctx.run_id, push_side_effect_key.value, ctx.branch);
        }

        var pr_side_effect_key_buf: [192]u8 = undefined;
        const pr_side_effect_key = try side_effect_keys.sideEffectKeyPrCreate(run_alloc, &pr_side_effect_key_buf, ctx.branch);
        defer pr_side_effect_key.deinit(run_alloc);
        const pr_claimed = try state.claimSideEffect(conn, ctx.run_id, pr_side_effect_key.value, ctx.branch);
        if (!pr_claimed) {
            pr_url = try getExistingPrUrl(run_alloc, conn, ctx.run_id);
            if (pr_url == null) {
                pr_url = try tryRecoverPrUrl(run_alloc, conn, token_cache, tenant_limiter, cancel_flag, deadline_ms, ctx);
                if (pr_url == null) return worker_runtime.WorkerError.SideEffectClaimedNoResult;
            }
        }

        if (pr_url == null) {
            try worker_runtime.ensureRunActive(cancel_flag, deadline_ms);
            log.info("pipeline.pr_create_start run_id={s} branch={s} repo={s}", .{ ctx.run_id, ctx.branch, ctx.repo_url });
            const pr_title = try std.fmt.allocPrint(run_alloc, "usezombie: {s}", .{ctx.spec_id});
            var pr_retry_ctx = PrRetryCtx{
                .alloc = run_alloc,
                .repo_url = ctx.repo_url,
                .branch = ctx.branch,
                .base_branch = ctx.default_branch,
                .title = pr_title,
                .body = ctx.final_stage_output,
                .github_token = github_token,
            };
            try tenant_limiter.acquireCancelable(ctx.tenant_id, "github_pr_create", 1.0, cancel_flag, deadline_ms);
            const created_pr = reliable.callWithDetail(
                []u8,
                &pr_retry_ctx,
                opCreatePr,
                prDetail,
                worker_runtime.retryOptionsForRun(@constCast(cancel_flag), deadline_ms, 2, 1_000, 10_000, "github_pr_create"),
            ) catch |pr_err| blk: {
                switch (onPrCreateFailure(pr_err, pr_retry_ctx.last_error_detail)) {
                    .lookup_existing_pr => {
                        const recovered = try git.findOpenPullRequestByHead(run_alloc, ctx.repo_url, ctx.branch, github_token, null);
                        if (recovered) |url| break :blk try run_alloc.dupe(u8, url);
                        if (indicatesClosedOrDeleted(pr_retry_ctx.last_error_detail)) {
                            return git.GitError.PrInvalidRequest;
                        }
                        return pr_err;
                    },
                    .retry => return pr_err,
                    .fail => return pr_err,
                }
            };
            pr_url = created_pr;
            try state.markSideEffectDone(conn, ctx.run_id, pr_side_effect_key.value, created_pr);
            log.info("pipeline.pr_created run_id={s} branch={s}", .{ ctx.run_id, ctx.branch });
        }
    }
    if (pr_url != null) {
        log.info("pipeline.pr_ensured run_id={s} branch={s}", .{ ctx.run_id, ctx.branch });
    }
    return pr_url orelse git.GitError.PrFailed;
}

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (haystack.len < needle.len) return false;

    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(haystack[i .. i + needle.len], needle)) return true;
    }
    return false;
}

test "onPushFailure asks to verify remote branch for concurrency races" {
    try std.testing.expectEqual(PushFailureAction.lookup_remote_branch, onPushFailure());
}

test "onPrCreateFailure retries transient GitHub failures" {
    try std.testing.expectEqual(PrCreateFailureAction.retry, onPrCreateFailure(git.GitError.PrRateLimited, null));
    try std.testing.expectEqual(PrCreateFailureAction.retry, onPrCreateFailure(git.GitError.PrServerError, null));
}

test "onPrCreateFailure looks up existing PR for invalid request races" {
    try std.testing.expectEqual(PrCreateFailureAction.lookup_existing_pr, onPrCreateFailure(git.GitError.PrInvalidRequest, "already exists"));
    try std.testing.expectEqual(PrCreateFailureAction.lookup_existing_pr, onPrCreateFailure(git.GitError.PrFailed, "unknown"));
}

test "onPrCreateFailure fails closed on auth failures" {
    try std.testing.expectEqual(PrCreateFailureAction.fail, onPrCreateFailure(git.GitError.PrAuthFailed, null));
}

test "indicatesClosedOrDeleted catches closed/deleted/empty-diff false positives" {
    try std.testing.expect(indicatesClosedOrDeleted("Pull request is closed"));
    try std.testing.expect(indicatesClosedOrDeleted("branch was deleted"));
    try std.testing.expect(indicatesClosedOrDeleted("No commits between base and head"));
    try std.testing.expect(!indicatesClosedOrDeleted("rate limit exceeded"));
}

test "detects commit and worktree edge cases from stderr/http details" {
    try std.testing.expect(indicatesNothingToCommit("nothing to commit, working tree clean"));
    try std.testing.expect(indicatesDirtyWorktree("error: Committing is not possible because you have unmerged files."));
    try std.testing.expect(indicatesPushRejected("! [rejected] HEAD -> branch (non-fast-forward)"));
}

test "detects PR conflict markers and avoids false positives" {
    try std.testing.expect(indicatesPrConflict("This pull request has conflicts"));
    try std.testing.expect(!indicatesPrConflict("pull request created successfully"));
}
