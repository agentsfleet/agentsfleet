//! Git operations — thin facade re-exporting all public symbols from
//! focused submodules.
//!
//! Callers continue to use `const ops = @import("../git/ops.zig")` and
//! access `ops.ensureBareClone`, `ops.createPullRequest`, etc., unchanged.
//!
//! Submodules:
//!   - `errors.zig`   — shared `GitError` error set
//!   - `validate.zig` — pure input-validation helpers
//!   - `command.zig`  — child-process execution (`CommandResources`, `run`)
//!   - `repo.zig`     — bare clone, worktree, commit, push, cleanup
//!   - `pr.zig`       — GitHub pull-request REST API calls

const errors_mod = @import("errors.zig");
const validate_mod = @import("validate.zig");
const command_mod = @import("command.zig");
const repo_mod = @import("repo.zig");
const pr_mod = @import("pr.zig");

// Re-export the shared error set so existing `ops.GitError` references work.
pub const GitError = errors_mod.GitError;

// validate.zig exports
pub const isSafeIdentifierSegment = validate_mod.isSafeIdentifierSegment;
pub const isSafeGitRef = validate_mod.isSafeGitRef;
pub const isSafeRelativePath = validate_mod.isSafeRelativePath;
pub const isSafeWorktreeDirName = validate_mod.isSafeWorktreeDirName;

// command.zig exports
pub const CommandResources = command_mod.CommandResources;
pub const sanitizedChildEnv = command_mod.sanitizedChildEnv;
pub const copyEnvIfPresent = command_mod.copyEnvIfPresent;
pub const run = command_mod.run;

// repo.zig exports
pub const WorktreeHandle = repo_mod.WorktreeHandle;
pub const RuntimeCleanupStats = repo_mod.RuntimeCleanupStats;
pub const ensureBareClone = repo_mod.ensureBareClone;
pub const createWorktree = repo_mod.createWorktree;
pub const removeWorktree = repo_mod.removeWorktree;
pub const cleanupRuntimeArtifacts = repo_mod.cleanupRuntimeArtifacts;
pub const commitFile = repo_mod.commitFile;
pub const push = repo_mod.push;
pub const remoteBranchExists = repo_mod.remoteBranchExists;
pub const getHeadSha = repo_mod.getHeadSha;

// pr.zig exports
pub const HttpResponseParts = pr_mod.HttpResponseParts;
pub const splitHttpResponse = pr_mod.splitHttpResponse;
pub const parseHttpStatus = pr_mod.parseHttpStatus;
pub const parseGitHubOwnerRepo = pr_mod.parseGitHubOwnerRepo;
pub const createPullRequest = pr_mod.createPullRequest;
pub const findOpenPullRequestByHead = pr_mod.findOpenPullRequestByHead;
pub const postPrComment = pr_mod.postPrComment;
