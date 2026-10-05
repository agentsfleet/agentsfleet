//! Which repositories a lease's workspace holds, and where each lands.
//!
//! The supervisor clones exactly these before the turn runs, the prompt says
//! where they are, and the `git` tool runs in the one it finds: all three ask
//! this module, so they cannot disagree about a lease.

use afd_wire::policy::ExecutionPolicy;

use crate::catalog::{EXEC_COMMAND, GIT, SHELL};
use crate::error::{self, Result};

/// The integration a repository's token is minted from; the daemon's
/// `afd_gate` spells its own constant alike.
pub const CREDENTIAL_GITHUB: &str = "github";
/// The tools that run processes in the workspace: a lease offering one gets
/// its bound repositories checked out.
const PROCESS_TOOLS: [&str; 3] = [SHELL.name(), EXEC_COMMAND.name(), GIT.name()];
/// The one character between a repository's owner and its name.
const OWNER_SEPARATOR: char = '/';
/// Path segments that would climb out of the workspace or stand still.
const DOT_SEGMENTS: [&str; 2] = [".", ".."];

/// One repository a lease's workspace holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkout<'p> {
    /// `owner/name`, as the binding spells it.
    pub repository: &'p str,
    /// The owner part.
    pub owner: &'p str,
    /// The name part, which is also its directory under the workspace.
    pub name: &'p str,
    /// The branch checked out; empty for a read binding, which names none, so
    /// the supervisor checks out the repository's default branch.
    pub base: &'p str,
}

/// The repositories a lease under `policy` gets checked out: every bound one
/// when an offered tool runs processes, none otherwise.
///
/// # Errors
/// A bound name that is not `owner/name` of plain path segments: the lease is
/// refused rather than cloned somewhere else.
pub fn checkouts<'p>(policy: &'p ExecutionPolicy<'_>) -> Result<Vec<Checkout<'p>>> {
    let runs_processes = policy
        .tools
        .iter()
        .any(|tool| PROCESS_TOOLS.contains(&tool.as_ref()));
    let Some(binding) = policy
        .repository_binding
        .as_ref()
        .filter(|_| runs_processes)
    else {
        return Ok(Vec::new());
    };
    binding
        .repositories
        .iter()
        .map(|repository| checkout(repository, &binding.base_branch))
        .collect()
}

/// `repository` at `base`, split into its owner and name.
fn checkout<'p>(repository: &'p str, base: &'p str) -> Result<Checkout<'p>> {
    let (owner, name) = repository
        .split_once(OWNER_SEPARATOR)
        .filter(|(owner, name)| segment(owner) && segment(name))
        .ok_or_else(|| error::invalid_repository(repository))?;
    Ok(Checkout {
        repository,
        owner,
        name,
        base,
    })
}

/// Whether `part` is one plain path segment of the characters an owner or a
/// repository name may hold on GitHub.
fn segment(part: &str) -> bool {
    !part.is_empty()
        && !DOT_SEGMENTS.contains(&part)
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
#[path = "repositories/tests.rs"]
mod tests;
