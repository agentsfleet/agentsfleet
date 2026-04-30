//! Git operations — thin facade re-exporting the live cleanup surface.
//!
//! Sole external caller: `cmd/preflight.zig` invokes `ops.cleanupRuntimeArtifacts`
//! to sweep stale worktrees and bare-repo prunes during preflight.
//!
//! Direct submodule:
//!   - `repo.zig` — runtime-artifact cleanup (transitively uses
//!                  `errors.zig`, `validate.zig`, `command.zig`).

const repo_mod = @import("repo.zig");

pub const RuntimeCleanupStats = repo_mod.RuntimeCleanupStats;
pub const cleanupRuntimeArtifacts = repo_mod.cleanupRuntimeArtifacts;
