//! Which repositories a lease's workspace holds, and where each lands.
//!
//! The supervisor clones exactly these before the turn runs, the prompt says
//! where they are, and the `git` tool runs in the one it finds: all three ask
//! this module, so they cannot disagree about a lease.

use afd_wire::policy::ExecutionPolicy;

use crate::catalog;
use crate::error::{self, Result};
use crate::runtime::Runtime;

/// The integration a repository's token is minted from; the daemon's
/// `afd_gate` spells its own constant alike.
pub const CREDENTIAL_GITHUB: &str = "github";
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
/// when an offered tool runs in the sandbox, where it works in the workspace,
/// and none for a lease that starts no sandbox.
///
/// # Errors
/// A bound name that is not `owner/name` of plain path segments, or two bound
/// names that would land in one directory: the lease is refused rather than
/// cloned somewhere else.
pub fn checkouts<'p>(policy: &'p ExecutionPolicy<'_>) -> Result<Vec<Checkout<'p>>> {
    let in_sandbox = policy.tools.iter().any(|tool| {
        catalog::published(tool).is_some_and(|entry| entry.runtime() == Runtime::Sandbox)
    });
    let Some(binding) = policy.repository_binding.as_ref().filter(|_| in_sandbox) else {
        return Ok(Vec::new());
    };
    let found: Vec<Checkout<'p>> = binding
        .repositories
        .iter()
        .map(|repository| checkout(repository, &binding.base_branch))
        .collect::<Result<_>>()?;
    match sharing_a_directory(&found) {
        Some((first, second)) => Err(error::shared_directory(first, second)),
        None => Ok(found),
    }
}

/// The first two of `found` that would land in one directory, if any do.
fn sharing_a_directory<'p>(found: &[Checkout<'p>]) -> Option<(&'p str, &'p str)> {
    found.iter().enumerate().find_map(|(index, first)| {
        found
            .get(index + 1..)?
            .iter()
            .find(|second| second.name == first.name)
            .map(|second| (first.repository, second.repository))
    })
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
