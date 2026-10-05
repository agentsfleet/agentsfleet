//! The prompt: the installed instructions as the system prompt, the trusted
//! repair context beneath them on a write-bound lease, where the workspace's
//! repositories are checked out, and the event's message as the first user
//! turn.
//!
//! The message is the event's `message` field when the request carries one as
//! a string, and the whole request otherwise, the fallback
//! `src/runner/child_exec_input.zig` defines.

use std::fmt;

use afd_wire::lease::LeasePayload;
use afd_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
use afd_wire::policy::{ExecutionPolicy, HttpMethod, RepositoryAccess};
use afr_tools::sandbox::{Checkout, checkouts};

/// The heading the installed instructions render under.
const INSTALLED_INSTRUCTIONS: &str = "## Installed instructions\n\n";
/// The heading the trusted repair context renders under.
const TRUSTED_REPAIR_CONTEXT: &str = "## Trusted repair context";
/// The heading the workspace's checkouts render under.
const WORKSPACE: &str = "## Workspace";
/// What separates two blocks of the system prompt.
const BLOCK_BREAK: &str = "\n\n";
/// The request field holding the event's message.
const FIELD_MESSAGE: &str = "message";

/// What a run asks the model first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prompt {
    /// The system prompt.
    pub(crate) instructions: String,
    /// The first user turn.
    pub(crate) message: String,
}

impl Prompt {
    /// The prompt for `lease`.
    pub(crate) fn new(lease: &LeasePayload<'_>) -> Self {
        let request = lease.event.request_json.as_ref();
        let message = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|value| {
                value
                    .get(FIELD_MESSAGE)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| request.to_owned());
        let installed = (!lease.instructions.is_empty())
            .then(|| format!("{INSTALLED_INSTRUCTIONS}{}", lease.instructions));
        let repair = RepairContext::of(&lease.policy).map(|context| context.to_string());
        let workspace = Workspace::of(&lease.policy).map(|workspace| workspace.to_string());
        let instructions = installed
            .into_iter()
            .chain(repair)
            .chain(workspace)
            .collect::<Vec<_>>()
            .join(BLOCK_BREAK);
        Self {
            instructions,
            message,
        }
    }
}

/// What a write-bound lease may publish.
///
/// Read from the daemon's binding and the ref rule it locked, never from the
/// event: the branch is the one value the model must not choose, and both
/// repairer bundles start by asking for it (`ci-repairer/SKILL.md` step 1).
struct RepairContext<'p> {
    /// The one repository the binding names.
    repository: &'p str,
    /// The branch the locked `git/refs` rule admits.
    branch: &'p str,
    /// The branch a Pull Request opens into.
    base: &'p str,
}

impl<'p> RepairContext<'p> {
    /// The context `policy` authorises; `None` for a read-bound or unbound
    /// lease, and for a write binding the daemon compiled no ref rule for.
    fn of(policy: &'p ExecutionPolicy<'_>) -> Option<Self> {
        let binding = policy
            .repository_binding
            .as_ref()
            .filter(|binding| binding.access == RepositoryAccess::Write)?;
        let [repository] = binding.repositories.as_slice() else {
            return None;
        };
        let refs = repository::path(repository, REFS_PATH);
        let branch = policy
            .http_origin_policies
            .iter()
            .flat_map(|origin| &origin.requests)
            .filter(|rule| rule.method == HttpMethod::Post && rule.path == refs)
            .flat_map(|rule| &rule.json_fields)
            .find(|field| field.name == FIELD_REF)?
            .string_value
            .as_deref()?
            .strip_prefix(REFS_HEADS)?;
        Some(Self {
            repository,
            branch,
            base: &binding.base_branch,
        })
    }
}

impl fmt::Display for RepairContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{TRUSTED_REPAIR_CONTEXT}\nrepository: {}\nrepair branch: {}\ntrusted base: {}",
            self.repository, self.branch, self.base
        )
    }
}

/// Where the supervisor checked the lease's repositories out, so the model
/// starts in them instead of looking for them.
struct Workspace<'p>(Vec<Checkout<'p>>);

impl<'p> Workspace<'p> {
    /// The checkouts `policy` gets; `None` when it gets none, and for a
    /// binding that does not parse, which the supervisor refused before the
    /// turn began.
    fn of(policy: &'p ExecutionPolicy<'_>) -> Option<Self> {
        checkouts(policy)
            .ok()
            .filter(|checkouts| !checkouts.is_empty())
            .map(Self)
    }
}

impl fmt::Display for Workspace<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{WORKSPACE}")?;
        for checkout in &self.0 {
            write!(
                f,
                "\n{} is checked out at ./{} on {}, with origin set",
                checkout.repository, checkout.name, checkout.base
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "prompt/tests.rs"]
mod tests;
