//! Which repositories a fleet's credentials may reach, and how far.
//!
//! # Why this is not the trigger's repository list
//!
//! `Trigger::Webhook::repositories` is an INGRESS binding — which repositories
//! may wake the fleet. This is EGRESS — which its minted token may touch.
//! Overloading one for the other would mean every repository allowed to
//! trigger a fleet was also one that fleet could write to. They are separate
//! types here for that reason, not for tidiness.
//!
//! # Why a half-declared binding is refused
//!
//! A list with no access level does not know how far to reach; an access level
//! with no list does not know what to reach. Either would have to fall back to
//! the App installation's full scope across every repository it covers — which
//! is exactly what declaring a binding exists to prevent. Absent entirely is
//! fine and means the mint refuses.

use serde::Serialize;

use crate::config::raw;
use crate::error::{ErrorKind, Result};

/// Why a binding was refused.
const REASON_HALF_DECLARED: &str =
    "`repositories` and `repository_access` are optional together, not separately";
/// See [`REASON_HALF_DECLARED`].
const REASON_READ_WITH_BASE: &str = "a read binding opens no Pull Request, so it takes no base";
/// See [`REASON_HALF_DECLARED`].
const REASON_WRITE_WITHOUT_BASE: &str = "a write binding must name the base it opens against";

/// How far a fleet's repository credentials reach.
pub type Access = raw::Access;

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
    /// The access level a gate row records and the GitHub mint scopes by.
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
}

impl RepositoryBinding {
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
    /// Behind `test-util` (`M-TEST-UTIL`) for the reason
    /// [`GatePolicy::from_parts`](crate::config::GatePolicy::from_parts) is:
    /// the production door is [`Self::parse`] and it is the one that
    /// VALIDATES — half-declared refused, an empty list refused. [`Reach`]
    /// already rules out a read with a base and a write without one, so a
    /// test cannot build that pairing here either.
    ///
    /// The consumer is the credential mint in the sibling crate, whose subject
    /// is what a GitHub token came back reaching. Driving those cases through a
    /// stored config document would make every one of them carry a JSON fixture
    /// whose parse is not what the test is about.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn from_parts(repositories: Vec<Box<str>>, reach: Reach) -> Self {
        Self {
            repositories: repositories.into_boxed_slice(),
            reach,
        }
    }

    /// Reads the binding out of an already-deserialized runtime block.
    ///
    /// # Errors
    /// [`Error::InvalidRepositoryBinding`] for a half-declared or empty
    /// binding, or [`Error::InvalidList`] for a malformed entry.
    pub(crate) fn parse(authored: &mut raw::Runtime) -> Result<Option<Self>> {
        let repositories = authored.repositories.take();
        let base = authored.repository_base.take();

        match (repositories, authored.repository_access, base) {
            (None, None, None) => Ok(None),
            (Some(list), Some(access), base) => Ok(Some(Self {
                // Bounded and shape-checked by the schema, including the
                // refusal of an empty list — a token scoped to nothing cannot
                // mint, so "names nothing" is not "every repository".
                repositories: list.into_iter().map(Into::into).collect(),
                reach: reach(access, base)?,
            })),
            _ => Err(ErrorKind::InvalidRepositoryBinding {
                reason: REASON_HALF_DECLARED,
            }
            .into()),
        }
    }
}

/// A binding as an approval gate RECORDS it.
///
/// # Why a recorded copy exists at all
///
/// The gate row keeps the reach a human actually approved, so the write mint
/// can compare it against the fleet's CURRENT config without trusting anything
/// a PATCH could reach. A config that grew a repository since the approval is
/// drift the
/// mint must refuse, and it can only see that drift against a stored copy.
///
/// # Why it is a serde type and not a writer
///
/// A writer that builds this object a byte at a time owns the escaping, the
/// separator placement and the optional trailing member, and each is a hazard
/// this derive does not have. What is fixed is the SHAPE: the mint reads these
/// three key names.
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
}

impl RepositoryBinding {
    /// This binding in the one shape a gate row stores it in.
    ///
    /// Borrowed, so recording costs no copy of the repository list — the row
    /// write serialises straight out of the config it resolved.
    #[must_use]
    pub fn recorded(&self) -> Recorded<'_> {
        Recorded {
            repositories: &self.repositories,
            access: self.reach.access(),
            base: self.reach.base(),
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
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::assertions_on_result_states,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Reach, RepositoryBinding};
    use crate::config::raw;
    use crate::error::Error;
    use crate::error::ErrorKind;
    use garde::Validate as _;

    /// Deserializes AND validates, so a schema-declared rule is in force here
    /// exactly as it is on the real path.
    fn parse(json: &str) -> Result<Option<RepositoryBinding>, Error> {
        let mut authored: raw::Runtime =
            serde_json::from_str(json).expect("test fixture is a runtime block");
        authored.validate()?;
        RepositoryBinding::parse(&mut authored)
    }

    #[test]
    fn an_undeclared_binding_is_absent_rather_than_an_error() {
        assert!(
            parse(r#"{"tools": []}"#)
                .expect("declaring nothing is legitimate")
                .is_none()
        );
    }

    #[test]
    fn a_list_without_an_access_level_is_refused() {
        let failure = parse(r#"{"repositories": ["a/b"]}"#)
            .expect_err("a list alone does not know how far to reach");

        assert!(
            matches!(failure.kind(), ErrorKind::InvalidRepositoryBinding { .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn an_access_level_without_a_list_is_refused() {
        assert!(parse(r#"{"repository_access": "read"}"#).is_err());
    }

    #[test]
    fn an_empty_list_is_refused_because_a_token_scoped_to_nothing_cannot_mint() {
        assert!(parse(r#"{"repositories": [], "repository_access": "read"}"#).is_err());
    }

    #[test]
    fn a_read_binding_takes_no_base() {
        let binding =
            parse(r#"{"repositories": ["agentsfleet/agentsfleet"], "repository_access": "read"}"#)
                .expect("a read binding is complete without a base")
                .expect("a binding was declared");

        assert_eq!(binding.reach(), &Reach::Read);
    }

    #[test]
    fn a_read_binding_that_names_a_base_is_refused() {
        assert!(parse(
            r#"{"repositories": ["a/b"], "repository_access": "read", "repository_base": "main"}"#
        )
        .is_err());
    }

    #[test]
    fn a_write_binding_authored_without_a_base_is_refused() {
        assert!(parse(r#"{"repositories": ["a/b"], "repository_access": "write"}"#).is_err());
    }

    #[test]
    fn a_write_binding_carries_its_base_and_records_it() {
        let binding = parse(
            r#"{"repositories": ["a/b"], "repository_access": "write", "repository_base": "dev"}"#,
        )
        .expect("a write binding with a base is complete")
        .expect("a binding was declared");

        assert_eq!(binding.reach(), &Reach::Write { base: "dev".into() });
        let recorded = serde_json::to_value(binding.recorded()).expect("a record serializes");
        assert_eq!(
            recorded,
            serde_json::json!({"repositories": ["a/b"], "access": "write", "base": "dev"})
        );
    }

    #[test]
    fn an_unknown_access_level_names_the_ones_that_exist() {
        let failure = serde_json::from_str::<raw::Runtime>(
            r#"{"repositories": ["a/b"], "repository_access": "admin"}"#,
        )
        .expect_err("`admin` is not an access level");

        let rendered = failure.to_string();
        assert!(
            rendered.contains("read") && rendered.contains("write"),
            "there is no third level, and the message says which two exist: {rendered}"
        );
    }

    #[test]
    fn a_branch_name_may_not_reach_outside_itself() {
        let binding = |base: &str| {
            parse(&format!(
                r#"{{"repositories": ["a/b"], "repository_access": "write", "repository_base": "{base}"}}"#
            ))
        };

        assert!(binding("main").is_ok());
        assert!(binding("release/2.0").is_ok());
        assert!(binding("../etc").is_err(), "`..` escapes the ref");
        assert!(
            binding("main@{1}").is_err(),
            "a reflog reference is not a branch"
        );
        assert!(binding("main.lock").is_err(), "`.lock` is git's own suffix");
        assert!(binding("main~1").is_err(), "`~` is a revision operator");
        assert!(binding("/main").is_err());
        assert!(binding("main/").is_err());
        assert!(binding("").is_err());
    }
}
