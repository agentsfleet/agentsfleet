//! A repository binding translated into provider-neutral lease rules.
//!
//! # Why a binding becomes an ALLOW-LIST of exact requests
//!
//! A minted GitHub token is scoped to repositories, which bounds WHAT a run can
//! reach but not WHAT IT CAN DO there. These rules are the second boundary: the
//! runner admits a request only if some rule names its method, its path, and —
//! where it matters — the exact values of the JSON fields that decide blast
//! radius. [`read`] is the breadth, bounded by a prefix; [`write`] is the
//! narrow set, bounded exactly.
//!
//! This is also where an approval becomes enforceable. The card told a human
//! "one branch, one draft Pull Request in the bound repository", and that
//! sentence is only true because no other request is admitted.
//!
//! # Three refusals, and each is a fleet misconfiguration
//!
//! A write binding with no repair branch, no base, or more than one repository
//! cannot be turned into rules that bound anything. They refuse rather than
//! default because every available default is a WIDENING: no branch would mean
//! any branch, no base would mean any base, and several repositories would mean
//! the single-repository rules apply to a repository nobody checked.

mod read;
mod write;

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use afd_wire::policy::{HttpOriginPolicy, HttpRequestRule};

/// The only host these rules govern.
pub const API_HOST: &str = "api.github.com";

/// The credential admitted at that host.
pub const CREDENTIAL_GITHUB: &str = "github";

/// Why a write binding could not be turned into rules that bound anything.
///
/// Three arms, and every one is a FLEET AUTHOR's mistake rather than an
/// operational fault — which is the distinction the caller needs and the reason
/// this is a type instead of a message. Each ends the event with something an
/// operator can act on; none is worth a retry, because nothing about the next
/// poll will be different.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Misconfigured {
    /// No branch was authorised for this lease to write on.
    #[error("a write binding needs the branch its lease authorised")]
    NoRepairBranch,
    /// The binding names no base for a Pull Request to open against.
    #[error("a write binding must name the base it opens against")]
    NoBaseBranch,
    /// The binding names none, or several, where the locked rules bound one.
    #[error("a write binding bounds exactly one repository")]
    NotExactlyOneRepository,
}

/// The rules a lease's repository binding authorises.
///
/// One origin policy, for [`API_HOST`], carrying every admitted request. A read
/// binding contributes two rules per repository; a write binding adds five more
/// for its single repository — and keeps its reads, because a fleet that can
/// push must still be able to look at what it is pushing to.
///
/// # Errors
/// [`Misconfigured`] for a write binding this cannot bound.
///
/// The error type is spelled explicitly rather than defaulting to the crate's,
/// which is the carve-out `RULE ERR-RS` names for a signature answering a
/// different error. It is deliberate: these are the fleet author's mistakes,
/// and the caller must tell them from a datastore fault WITHOUT matching on a
/// message. One ends the event naming the key to fix; the other is retried.
pub fn build<'a>(
    binding: &RepositoryBinding,
    repair_branch: Option<&str>,
) -> crate::Result<Vec<HttpOriginPolicy<'a>>, Misconfigured> {
    let mut requests: Vec<HttpRequestRule<'a>> = binding
        .repositories()
        .iter()
        .flat_map(|repository| read::rules(repository))
        .collect();

    if binding.access() == Access::Write {
        requests.extend(write::rules(binding, repair_branch)?);
    }

    Ok(vec![HttpOriginPolicy {
        host: API_HOST.into(),
        credential_names: vec![CREDENTIAL_GITHUB.into()],
        requests,
    }])
}
#[cfg(test)]
mod tests;
