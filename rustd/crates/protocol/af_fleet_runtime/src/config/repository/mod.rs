//! Which repositories a fleet's GitHub grant reaches, and how far.
//!
//! # Why this is not the trigger's repository list
//!
//! `Trigger::Webhook::repositories` is an INGRESS binding — which repositories
//! may wake the fleet. This is EGRESS — which its minted token may touch.
//! Overloading one for the other would mean every repository allowed to
//! trigger a fleet was also one that fleet could write to. They are separate
//! types here for that reason, not for tidiness.
//!
//! # Why a half-declared scope is refused
//!
//! A list with no access level does not know how far to reach; an access level
//! with no list does not know what to reach. Either would have to fall back to
//! the App installation's full scope across every repository it covers — which
//! is exactly what a scope exists to prevent. A GitHub grant always names one.

use garde::Validate as _;
use serde::{Deserialize, Serialize};

use crate::config::raw;
use crate::error::{ErrorKind, Result};

/// Why a scope was refused.
const REASON_EXISTING_PR_WRITE: &str =
    "existing PR publication requires an explicit repository write binding";
/// See [`REASON_EXISTING_PR_WRITE`].
const REASON_EXISTING_PR_WORKFLOWS: &str =
    "existing PR publication requires unique validation workflow digests";
/// See [`REASON_EXISTING_PR_WRITE`].
const REASON_HALF_DECLARED: &str = "a GitHub scope names both `repositories` and `access`";
/// See [`REASON_EXISTING_PR_WRITE`].
const REASON_READ_WITH_BASE: &str = "a read binding opens no Pull Request, so it takes no base";
/// See [`REASON_EXISTING_PR_WRITE`].
const REASON_WRITE_WITHOUT_BASE: &str = "a write binding must name the base it opens against";

/// Publication authority a person approved with the GitHub grant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryPublication {
    /// Existing ordinary behavior: one event-owned branch and draft PR.
    #[default]
    EventDraft,
    /// Native task publication to an exact human-assigned existing PR.
    ExistingPr,
}

/// Exact workflow content a person approved as validation-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ValidationWorkflow {
    /// Only a direct YAML file inside GitHub's workflow directory.
    #[garde(
        length(bytes, min = 23, max = 255),
        pattern(r"^\.github/workflows/[A-Za-z0-9_-]+\.ya?ml$")
    )]
    pub path: String,
    /// SHA-256 of the complete reviewed UTF-8 workflow text.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]{64}$"))]
    pub sha256: String,
}

/// How far a fleet's repository credentials reach.
pub use crate::config::raw::Access;

/// How far a binding reaches, with a write's base carried by the write itself.
///
/// A read binding with a base, or a write binding without one, cannot be
/// built: [`RepositoryBinding::parse`] refuses both at the boundary, so no
/// reader below it re-checks the pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Fetch history, and nothing more.
    Read,
    /// Everything `Read` allows, plus opening a draft Pull Request.
    Write {
        /// The trusted branch every Pull Request opens against.
        base: Box<str>,
    },
}

impl Reach {
    /// The access level a grant records and the GitHub mint scopes by.
    #[must_use]
    pub const fn access(&self) -> Access {
        match self {
            Self::Read => Access::Read,
            Self::Write { .. } => Access::Write,
        }
    }

    /// The base a write opens against; a read has none.
    #[must_use]
    pub fn base(&self) -> Option<&str> {
        match self {
            Self::Read => None,
            Self::Write { base } => Some(base.as_ref()),
        }
    }
}

/// Which repositories a fleet may reach, and how far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryBinding {
    /// The repositories, as `owner/name`.
    repositories: Box<[Box<str>]>,
    /// How far the reach goes, and against which base a write opens.
    reach: Reach,
    publication: RepositoryPublication,
    validation_workflows: Box<[ValidationWorkflow]>,
}

impl RepositoryBinding {
    /// Whether only the native publisher may advance an assigned existing PR.
    #[must_use]
    pub const fn publication(&self) -> RepositoryPublication {
        self.publication
    }

    /// Validation-only content a person approved with the grant.
    #[must_use]
    pub fn validation_workflows(&self) -> &[ValidationWorkflow] {
        &self.validation_workflows
    }

    /// Narrows a native runner-visible token without widening repository scope.
    #[must_use]
    pub fn read_only(&self) -> Self {
        Self {
            repositories: self.repositories.clone(),
            reach: Reach::Read,
            publication: self.publication,
            validation_workflows: Box::default(),
        }
    }

    /// Exact repository evidence reads never retain a fleet's wider token scope.
    #[must_use]
    pub fn read_repository(&self, repository: &str) -> Option<Self> {
        let held = self
            .repositories
            .iter()
            .find(|held| held.eq_ignore_ascii_case(repository))?;
        Some(Self {
            repositories: vec![held.clone()].into_boxed_slice(),
            reach: Reach::Read,
            publication: self.publication,
            validation_workflows: Box::default(),
        })
    }

    /// The repositories, as `owner/name`.
    #[must_use]
    pub fn repositories(&self) -> &[Box<str>] {
        &self.repositories
    }

    /// How far the reach goes, and against which base a write opens.
    #[must_use]
    pub const fn reach(&self) -> &Reach {
        &self.reach
    }

    /// The access level alone, for readers that scope by it.
    #[must_use]
    pub const fn access(&self) -> Access {
        self.reach.access()
    }

    /// Builds a binding directly from its parts.
    ///
    /// Behind `test-util` (`M-TEST-UTIL`): the production door is
    /// [`Self::parse`] and it is the one that VALIDATES — half-declared
    /// refused, an empty list refused. [`Reach`] already rules out a read with
    /// a base and a write without one, so a test cannot build that pairing
    /// here either. The consumer is the credential mint, whose subject is what
    /// a GitHub token came back reaching, not how a scope is spelled.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn from_parts(repositories: Vec<Box<str>>, reach: Reach) -> Self {
        Self {
            repositories: repositories.into_boxed_slice(),
            reach,
            publication: RepositoryPublication::EventDraft,
            validation_workflows: Box::default(),
        }
    }

    /// Reads a GitHub scope: the JSON a run's access request carries, or the
    /// one a grant stored from [`Self::recorded`].
    ///
    /// # Errors
    /// A shape failure or an unknown key, a bound the scope broke, or
    /// `InvalidRepositoryBinding` for a half-declared scope or a pairing of
    /// access and base that cannot hold.
    pub fn parse(scope: &str) -> Result<Self> {
        let authored: raw::Scope = raw::strict(scope)?;
        authored.validate()?;
        Self::resolve(authored)
    }

    /// Applies the rules serde and garde cannot know.
    fn resolve(authored: raw::Scope) -> Result<Self> {
        let publication = authored.publication.unwrap_or_default();
        let workflows = authored
            .validation_workflows
            .map_or_default(|workflows| workflows.0);
        if publication == RepositoryPublication::ExistingPr
            && authored.access != Some(Access::Write)
        {
            return Err(ErrorKind::InvalidRepositoryBinding {
                reason: REASON_EXISTING_PR_WRITE,
            }
            .into());
        }
        if (publication == RepositoryPublication::ExistingPr) == workflows.is_empty()
            || workflows.iter().enumerate().any(|(index, entry)| {
                workflows
                    .iter()
                    .take(index)
                    .any(|other| other.path == entry.path)
            })
        {
            return Err(ErrorKind::InvalidRepositoryBinding {
                reason: REASON_EXISTING_PR_WORKFLOWS,
            }
            .into());
        }

        let (Some(list), Some(access)) = (authored.repositories, authored.access) else {
            return Err(ErrorKind::InvalidRepositoryBinding {
                reason: REASON_HALF_DECLARED,
            }
            .into());
        };
        Ok(Self {
            // Bounded and shape-checked by the schema, including the refusal
            // of an empty list — a token scoped to nothing cannot mint, so
            // "names nothing" is not "every repository".
            repositories: list.into_iter().map(Into::into).collect(),
            reach: reach(access, authored.base)?,
            publication,
            validation_workflows: workflows.into_boxed_slice(),
        })
    }
}

/// A binding as a grant RECORDS it: the scope a person approved, which
/// [`RepositoryBinding::parse`] reads back.
///
/// # Why it is a serde type and not a writer
///
/// A writer that builds this object a byte at a time owns the escaping, the
/// separator placement and the optional trailing member, and each is a hazard
/// this derive does not have. What is fixed is the SHAPE: the parse reads
/// these key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Recorded<'a> {
    /// The repositories, verbatim.
    ///
    /// Case is preserved rather than normalised: the MATCHER owns
    /// case-insensitivity, because that is where GitHub's own comparison rules
    /// apply, and a record that normalised would no longer say what was
    /// approved.
    repositories: &'a [Box<str>],
    /// How far the approved reach goes.
    access: Access,
    /// The base a write binding opens against.
    ///
    /// Omitted rather than null when absent, which is the shape the mint's
    /// matcher reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publication: Option<RepositoryPublication>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    validation_workflows: &'a [ValidationWorkflow],
}

impl RepositoryBinding {
    /// This binding in the one shape a grant stores it in.
    ///
    /// Borrowed, so recording costs no copy of the repository list — the row
    /// write serialises straight out of the binding it resolved.
    #[must_use]
    pub fn recorded(&self) -> Recorded<'_> {
        Recorded {
            repositories: &self.repositories,
            access: self.reach.access(),
            base: self.reach.base(),
            publication: (self.publication == RepositoryPublication::ExistingPr)
                .then_some(self.publication),
            validation_workflows: &self.validation_workflows,
        }
    }
}

/// Pairs the authored access level with its base, refusing either mismatch.
fn reach(access: Access, authored: Option<String>) -> Result<Reach> {
    let refuse = |reason| ErrorKind::InvalidRepositoryBinding { reason }.into();

    match (access, authored) {
        (Access::Read, None) => Ok(Reach::Read),
        (Access::Read, Some(_)) => Err(refuse(REASON_READ_WITH_BASE)),
        (Access::Write, None) => Err(refuse(REASON_WRITE_WITHOUT_BASE)),
        (Access::Write, Some(base)) => Ok(Reach::Write { base: base.into() }),
    }
}

#[cfg(test)]
mod tests;
