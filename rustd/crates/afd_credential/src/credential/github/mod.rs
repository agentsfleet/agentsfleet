//! The GitHub App half of the mint: an App JWT exchanged for a scoped
//! installation token.
//!
//! # Two REST calls, written here
//!
//! The App JWT — `iss`, `iat`, `exp`, the RS256 signature — is one
//! `jsonwebtoken::encode`, and the two calls are plain requests over the
//! broker's shared `reqwest` client ([`api`]). Owning them is what keeps the
//! security-relevant parts in view: the token request is ALWAYS posted with
//! [`ScopedRequest`] as its body, because an empty body means the
//! installation's FULL scope, and no client method that defaults to one is in
//! reach.
//!
//! # Why the RESPONSE is read through an open map
//!
//! A typed permission model with a field per permission and no catch-all would
//! do two harmful things at once: make a permission it does not model, such as
//! `pull_requests`, unreadable, and silently DROP any permission GitHub granted
//! that the struct does not name. A check written against it would pass while
//! the token carried `administration: write`, which is the exact overreach the
//! check exists to refuse — the typed model would make it invisible.
//!
//! So the response is read into [`Granted`], whose permission map is open.
//! An unmodelled permission arrives as [`Permission::Unknown`], which is
//! greater than [`Permission::Read`] and therefore refused.

use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroU64;

use afd_fleet_runtime::config::RepositoryBinding;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod api;
mod exchange;
mod request;

pub use self::exchange::{Exchange, mint};
pub use self::request::{Installed, ScopedRequest};

/// The one permission GitHub attaches to every installation token unasked,
/// always at read. [`Granted::verify`] admits it and no other stranger.
const AMBIENT_READ: &str = "metadata";

/// One repository permission a mint may ask GitHub for.
///
/// A closed set: a permission this enum does not name cannot be requested,
/// because it cannot be written down (`M-STRONG-TYPES`). The response side
/// keeps GitHub's own strings, because [`Granted::verify`] has to SEE a name it
/// does not know in order to refuse it. Serialised through [`Self::as_str`], so
/// the name on the wire and the name the response is checked by are one
/// spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(into = "&'static str")]
pub enum GithubPermission {
    /// Workflow runs, their jobs and the job-log requests: the CI evidence.
    Actions,
    /// Check runs and their annotations: where a failed assertion is quoted.
    Checks,
    /// Repository files, and at write the Git objects and ref a repair pushes.
    Contents,
    /// Opening the draft Pull Request a repair proposes. Write mints only.
    PullRequests,
}

impl GithubPermission {
    /// The name GitHub spells this permission with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Actions => "actions",
            Self::Checks => "checks",
            Self::Contents => "contents",
            Self::PullRequests => "pull_requests",
        }
    }
}

impl From<GithubPermission> for &'static str {
    fn from(permission: GithubPermission) -> Self {
        permission.as_str()
    }
}

/// How far one permission reaches.
///
/// Ordered, and that ordering is the whole check: "granted more than was asked
/// for" is `granted > requested`, a comparison the compiler derives, rather
/// than a chain of string equalities that has to be read to be believed.
///
/// [`Self::Unknown`] is last on purpose. A level GitHub introduces after this
/// was written sorts ABOVE write, so it is refused rather than admitted — an
/// unrecognised permission level must never be the permissive branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Fetch, and nothing more.
    Read,
    /// What opening a draft Pull Request needs.
    Write,
    /// Administrative reach. Never requested by this daemon.
    Admin,
    /// A level this daemon does not model.
    ///
    /// `#[serde(other)]` is what makes an unknown level VISIBLE. Without it a
    /// value outside this enum fails the whole deserialisation, and a caller
    /// that treated a parse failure as "no permissions granted" would be
    /// admitting the token it could not read.
    #[serde(other)]
    Unknown,
}

/// One repository a minted token turned out to reach.
#[derive(Debug, Deserialize)]
struct Reached {
    /// The QUALIFIED `owner/name`, which is what makes the check meaningful:
    /// the request could only name the bare half.
    full_name: String,
}

/// What GitHub actually minted.
///
/// Deserialised into this rather than a typed permission model — see the
/// module header for why such a model cannot be used safely here.
#[derive(Debug, Deserialize)]
pub struct Granted {
    /// The installation token itself.
    pub token: String,
    /// When it stops working, as GitHub spells it (RFC 3339).
    pub expires_at: Option<String>,
    /// Every permission granted, INCLUDING ones this daemon does not model.
    #[serde(default)]
    permissions: BTreeMap<String, Permission>,
    /// The repositories the token reaches.
    ///
    /// `Option`, and a `None` is refused rather than assumed: a request that
    /// named repositories is answered with the ones it was granted, so a
    /// missing array means something happened this code does not model.
    repositories: Option<Vec<Reached>>,
}

/// Why a minted token was refused after the exchange.
///
/// Separate from a mint FAILURE: the exchange succeeded and GitHub handed back
/// a working credential. What failed is that the credential does not match what
/// the fleet declared, so it is discarded rather than delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overreach {
    /// The token reaches a repository that was never declared, or misses one
    /// that was — the bare-name mis-scope this check exists for.
    Repositories,
    /// The token carries a permission, or a level, beyond what was asked for.
    Permissions,
    /// The response described no reach at all.
    ///
    /// Refused rather than assumed, for the reason [`Self::Repositories`] is:
    /// unknown reach must never be the permissive branch.
    Unstated,
}

impl Granted {
    /// Whether this token reaches exactly what was declared, and no further.
    ///
    /// # Errors
    /// Refuses a token whose reach or permissions exceed — or fall short of —
    /// the declaration, and one that states neither.
    pub fn verify(
        &self,
        binding: &RepositoryBinding,
        requested: &BTreeMap<GithubPermission, Permission>,
    ) -> Result<(), Overreach> {
        self.verify_repositories(binding)?;
        self.verify_permissions(requested)
    }

    /// Set equality between what was declared and what was granted.
    ///
    /// Both directions in one comparison, because both matter: a token reaching
    /// MORE than was declared is the mis-scope, and one reaching LESS fails
    /// later at the vendor where the fleet has no local explanation for it.
    /// Case-folded because GitHub owners and repository names are.
    fn verify_repositories(&self, binding: &RepositoryBinding) -> Result<(), Overreach> {
        let Some(reached) = self.repositories.as_ref() else {
            return Err(Overreach::Unstated);
        };
        let reached: HashSet<String> = reached
            .iter()
            .map(|repository| repository.full_name.to_lowercase())
            .collect();
        let declared: HashSet<String> = binding
            .repositories()
            .iter()
            .map(|repository| repository.to_lowercase())
            .collect();
        if reached == declared {
            Ok(())
        } else {
            Err(Overreach::Repositories)
        }
    }

    /// Every granted permission is [`AMBIENT_READ`] at read, or exactly what
    /// was asked for.
    ///
    /// Any other unrequested name is refused at every level, read included:
    /// an unasked `administration: read` still shows the token's holder a
    /// repository's settings and collaborators, which no fleet declared.
    fn verify_permissions(
        &self,
        requested: &BTreeMap<GithubPermission, Permission>,
    ) -> Result<(), Overreach> {
        if self.permissions.is_empty() {
            return Err(Overreach::Unstated);
        }
        // Nothing granted beyond GitHub's own `metadata` read except exactly
        // what was asked for, at exactly the level it was asked for.
        let within_request = self.permissions.iter().all(|(name, granted)| {
            (name == AMBIENT_READ && *granted == Permission::Read)
                || requested
                    .iter()
                    .any(|(permission, want)| permission.as_str() == name && want == granted)
        });
        // And everything asked for was actually granted, so a token quietly
        // NARROWER than the declaration is refused here rather than at the
        // vendor.
        let fully_granted = requested
            .iter()
            .all(|(permission, want)| self.permissions.get(permission.as_str()) == Some(want));
        if within_request && fully_granted {
            Ok(())
        } else {
            Err(Overreach::Permissions)
        }
    }
}

#[cfg(test)]
mod tests;

/// The handle field that names which installation to mint for.
const FIELD_INSTALLATION_ID: &str = "installation_id";

/// The installation a stored GitHub handle names, if it names one readably.
///
/// The one reader of that field. The exchange mints for what this returns,
/// and the lease plane checks the workspace owns the same installation before
/// any exchange runs — read in two places, a handle could pass the check as one
/// installation and be minted as another.
#[must_use]
pub fn installation(handle: &Value) -> Option<u64> {
    handle
        .as_object()
        .and_then(|fields| fields.get(FIELD_INSTALLATION_ID))
        .and_then(installation_id)
}

/// The installation id, however the handle spells it.
///
/// GitHub's id is numeric and a stored handle may carry it either as a JSON
/// number or as the string a form posted. Both are accepted, and anything else
/// is not — a handle whose installation is an object or an array names no
/// installation at all.
fn installation_id(stored: &Value) -> Option<u64> {
    match stored {
        Value::Number(number) => number
            .as_u64()
            .and_then(NonZeroU64::new)
            .map(NonZeroU64::get),
        Value::String(text) => text.parse::<NonZeroU64>().ok().map(NonZeroU64::get),
        _not_an_id => None,
    }
}
