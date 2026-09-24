//! What a mint asks GitHub for: the fleet's declared reach, trimmed to what
//! the installation can grant.

use std::collections::BTreeMap;

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use serde::{Deserialize, Serialize};

use super::{GithubPermission, Permission};

/// The CI evidence a failed run leaves: its runs, jobs and job-log requests,
/// and its check annotations. Asked for at read, and only where held.
const EVIDENCE_READS: [GithubPermission; 2] = [GithubPermission::Actions, GithubPermission::Checks];

/// What a read binding's own work needs: the repository's files.
///
/// No `pull_requests` entry at all, because the absence is the read scope.
const READ_REACH: [(GithubPermission, Permission); 1] =
    [(GithubPermission::Contents, Permission::Read)];

/// What a repair needs: the Git objects and ref it pushes, and the draft Pull
/// Request it opens. Never `workflows`, so a repair cannot change what CI runs.
const WRITE_REACH: [(GithubPermission, Permission); 2] = [
    (GithubPermission::Contents, Permission::Write),
    (GithubPermission::PullRequests, Permission::Write),
];

/// The permissions an installation was granted, read before the token is
/// asked for.
///
/// A token cannot carry a permission its App was never granted, and
/// [`super::Granted::verify`] refuses one narrower than its request, so an
/// evidence read the installation lacks would fail every mint rather than
/// just the read. Kept as GitHub's own strings for the reason `Granted` keeps
/// them: a map of names this daemon does not model still deserialises.
#[derive(Debug, Deserialize)]
pub struct Installed {
    #[serde(default)]
    permissions: BTreeMap<String, Permission>,
}

impl Installed {
    /// Whether a read of `permission` can be granted.
    ///
    /// A level this daemon does not model is treated as not held: asking for
    /// it risks the whole mint, and leaving it out costs one 403 the fleet can
    /// report.
    fn holds(&self, permission: GithubPermission) -> bool {
        self.permissions
            .get(permission.as_str())
            .is_some_and(|level| *level != Permission::Unknown)
    }
}

/// The body that narrows an installation token to one fleet's declared reach.
///
/// # Repositories go by BARE name, and that is GitHub's rule, not a choice
///
/// GitHub scopes an installation token by repository name WITHIN the
/// installation's own account, so the owner never reaches the wire. A binding
/// naming `acme/payments` is therefore sent as `payments`, and GitHub will
/// happily grant `<installed-account>/payments` if a repository by that bare
/// name exists there. It cannot cross a tenant — an installation belongs to one
/// account — but it is a real mis-scope inside an operator's own installation,
/// and nothing on the request side can prevent it.
///
/// That is why [`super::Granted::verify`] exists and why it checks the RESPONSE.
#[derive(Debug, Serialize)]
pub struct ScopedRequest {
    /// Bare repository names, owner stripped.
    repositories: Vec<String>,
    /// Exactly the permissions this access level needs, and no others.
    permissions: BTreeMap<GithubPermission, Permission>,
}

impl ScopedRequest {
    /// The narrowest request that satisfies `binding` on this installation.
    ///
    /// The binding's own reach is asked for whether `installed` holds it or
    /// not, so a repair the installation cannot make fails at the mint instead
    /// of quietly narrowing. [`EVIDENCE_READS`] are asked for only where held:
    /// a missing one leaves the fleet's CI read to answer 403, which the fleet
    /// can report.
    #[must_use]
    pub fn for_binding(binding: &RepositoryBinding, installed: &Installed) -> Self {
        let reach: &[(GithubPermission, Permission)] = match binding.access() {
            Access::Read => &READ_REACH,
            Access::Write => &WRITE_REACH,
        };
        let permissions = EVIDENCE_READS
            .into_iter()
            .filter(|permission| installed.holds(*permission))
            .map(|permission| (permission, Permission::Read))
            .chain(reach.iter().copied())
            .collect();
        Self {
            repositories: binding
                .repositories()
                .iter()
                .map(|repository| bare_name(repository))
                .collect(),
            permissions,
        }
    }

    /// What this request asked for, for the response to be checked against.
    #[must_use]
    pub const fn permissions(&self) -> &BTreeMap<GithubPermission, Permission> {
        &self.permissions
    }
}

/// The bare repository name GitHub scopes by, from a qualified `owner/name`.
///
/// Splits on the LAST separator: a repository name cannot contain one, so
/// whatever follows it is the name even when an owner does something unusual.
fn bare_name(qualified: &str) -> String {
    qualified
        .rsplit_once('/')
        .map_or_else(|| qualified.to_owned(), |(_owner, name)| name.to_owned())
}
