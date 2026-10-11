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

/// Why a write binding names no branch for its lease to write on.
pub const DETAIL_NO_REPAIR_BRANCH: &str = "a write binding needs the branch its lease authorised";

/// Why a write binding names no base for a Pull Request to open against.
pub const DETAIL_NO_BASE_BRANCH: &str = "a write binding must name the base it opens against";

/// Why a write binding names none, or several, where the locked rules bound one.
pub const DETAIL_NOT_EXACTLY_ONE_REPOSITORY: &str = "a write binding bounds exactly one repository";

/// The rules a lease's repository binding authorises.
///
/// One origin policy, for [`API_HOST`], carrying every admitted request. A read
/// binding contributes two rules per repository; a write binding adds five more
/// for its single repository — and keeps its reads, because a fleet that can
/// push must still be able to look at what it is pushing to.
///
/// # Errors
/// A rejection, [`crate::Error::is_rejected`], for a write binding this cannot
/// bound. These are the fleet author's mistakes, and the caller tells them from
/// a datastore fault by that predicate rather than by a message: a rejection
/// ends the event with its [`crate::Error::detail`] naming the key to fix, and
/// any other kind is retried.
pub fn build<'a>(
    binding: &RepositoryBinding,
    repair_branch: Option<&str>,
) -> crate::Result<Vec<HttpOriginPolicy<'a>>> {
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
