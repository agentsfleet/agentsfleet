//! Shared error set for all git submodules.
//!
//! This file exists solely to break the import cycle that would arise if
//! command.zig, repo.zig, and pr.zig all imported GitError from ops.zig while
//! ops.zig re-exports from those same files.
//!
//! Callers that currently do `ops.GitError` continue to work because ops.zig
//! re-exports this via `pub const GitError = @import("errors.zig").GitError`.

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
