//! Git operations — thin facade re-exporting the live cleanup surface.
//!
//! Sole external caller: `cmd/preflight.zig` invokes `ops.cleanupRuntimeArtifacts`
//! to sweep stale worktrees and bare-repo prunes during preflight.
//!
//! Submodules:
//!   - `errors.zig`   — shared `GitError` error set (used by `command.run`)
//!   - `validate.zig` — pure input-validation helpers used by repo cleanup
//!   - `command.zig`  — child-process execution
//!   - `repo.zig`     — runtime-artifact cleanup

const repo_mod = @import("repo.zig");

pub const RuntimeCleanupStats = repo_mod.RuntimeCleanupStats;
pub const cleanupRuntimeArtifacts = repo_mod.cleanupRuntimeArtifacts;
