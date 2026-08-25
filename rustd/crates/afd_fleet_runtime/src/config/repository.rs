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

use crate::config::raw;
use crate::error::{Error, Result};

/// Why a binding was refused.
const REASON_HALF_DECLARED: &str =
    "`repositories` and `repository_access` are optional together, not separately";
/// See [`REASON_HALF_DECLARED`].
const REASON_READ_WITH_BASE: &str = "a read binding opens no Pull Request, so it takes no base";
/// See [`REASON_HALF_DECLARED`].
const REASON_WRITE_WITHOUT_BASE: &str = "a write binding must name the base it opens against";

/// How far a fleet's repository credentials reach.
pub type Access = raw::Access;

/// Whether the document being read is being authored or was already stored.
///
/// The distinction exists for one shape and no other: write bindings saved
/// before `repository_base` existed have no base to read. Authoring refuses
/// them, so nobody creates another; stored admits them, so the lease path can
/// surface a durable, actionable upgrade refusal instead of failing the whole
/// parse and telling an operator nothing about which fleet to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A document being written now.
    Authoring,
    /// A document read back out of the datastore.
    Stored,
}

/// Which repositories a fleet may reach, and how far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryBinding {
    /// The repositories, as `owner/name`.
    repositories: Box<[Box<str>]>,
    /// How far the reach goes.
    access: Access,
    /// The trusted base a write binding opens against.
    ///
    /// `None` on a read binding, and on a stored write binding saved before
    /// this key existed — which the lease path refuses with a typed message.
    base_branch: Option<Box<str>>,
}

impl RepositoryBinding {
    /// The repositories, as `owner/name`.
    #[must_use]
    pub fn repositories(&self) -> &[Box<str>] {
        &self.repositories
    }

    /// How far the reach goes.
    #[must_use]
    pub const fn access(&self) -> Access {
        self.access
    }

    /// The trusted base a write binding opens against.
    #[must_use]
    pub fn base_branch(&self) -> Option<&str> {
        self.base_branch.as_deref()
    }

    /// Reads the binding out of an already-deserialized runtime block.
    ///
    /// # Errors
    /// [`Error::InvalidRepositoryBinding`] for a half-declared or empty
    /// binding, or [`Error::InvalidList`] for a malformed entry.
    pub(crate) fn parse(authored: &mut raw::Runtime, mode: Mode) -> Result<Option<Self>> {
        let repositories = authored.repositories.take();
        let base = authored.repository_base.take();

        match (repositories, authored.repository_access, base) {
            (None, None, None) => Ok(None),
            (Some(list), Some(access), base) => Ok(Some(Self {
                // Bounded and shape-checked by the schema, including the
                // refusal of an empty list — a token scoped to nothing cannot
                // mint, so "names nothing" is not "every repository".
                repositories: list.into_iter().map(Into::into).collect(),
                base_branch: base_branch(access, base, mode)?,
                access,
            })),
            _ => Err(Error::InvalidRepositoryBinding {
                reason: REASON_HALF_DECLARED,
            }),
        }
    }
}

/// Resolves the base branch for `access` under `mode`.
fn base_branch(access: Access, authored: Option<String>, mode: Mode) -> Result<Option<Box<str>>> {
    let refuse = |reason| Error::InvalidRepositoryBinding { reason };

    match (access, authored, mode) {
        // Two ways to legitimately carry no base: a read binding never has
        // one, and a stored write binding predates the key. The lease path
        // tells them apart by the access level, which it already holds.
        (Access::Read, None, _) | (Access::Write, None, Mode::Stored) => Ok(None),
        (Access::Read, Some(_), _) => Err(refuse(REASON_READ_WITH_BASE)),
        (Access::Write, None, Mode::Authoring) => Err(refuse(REASON_WRITE_WITHOUT_BASE)),
        // Its SHAPE was proved by the schema; what is decided here is only
        // whether a base belongs at all, which depends on access and mode.
        (Access::Write, Some(base), _) => Ok(Some(base.into())),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::assertions_on_result_states,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Access, Mode, RepositoryBinding};
    use crate::config::raw;
    use crate::error::Error;
    use garde::Validate as _;

    /// Deserializes AND validates, so a schema-declared rule is in force here
    /// exactly as it is on the real path.
    fn parse(json: &str, mode: Mode) -> Result<Option<RepositoryBinding>, Error> {
        let mut authored: raw::Runtime =
            serde_json::from_str(json).expect("test fixture is a runtime block");
        authored.validate()?;
        RepositoryBinding::parse(&mut authored, mode)
    }

    #[test]
    fn an_undeclared_binding_is_absent_rather_than_an_error() {
        assert!(
            parse(r#"{"tools": []}"#, Mode::Authoring)
                .expect("declaring nothing is legitimate")
                .is_none()
        );
    }

    #[test]
    fn a_list_without_an_access_level_is_refused() {
        let failure = parse(r#"{"repositories": ["a/b"]}"#, Mode::Authoring)
            .expect_err("a list alone does not know how far to reach");

        assert!(
            matches!(failure, Error::InvalidRepositoryBinding { .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn an_access_level_without_a_list_is_refused() {
        assert!(parse(r#"{"repository_access": "read"}"#, Mode::Authoring).is_err());
    }

    #[test]
    fn an_empty_list_is_refused_because_a_token_scoped_to_nothing_cannot_mint() {
        assert!(
            parse(
                r#"{"repositories": [], "repository_access": "read"}"#,
                Mode::Authoring
            )
            .is_err()
        );
    }

    #[test]
    fn a_read_binding_takes_no_base() {
        let binding = parse(
            r#"{"repositories": ["agentsfleet/agentsfleet"], "repository_access": "read"}"#,
            Mode::Authoring,
        )
        .expect("a read binding is complete without a base")
        .expect("a binding was declared");

        assert_eq!(binding.access(), Access::Read);
        assert_eq!(binding.base_branch(), None);
    }

    #[test]
    fn a_read_binding_that_names_a_base_is_refused() {
        assert!(parse(
            r#"{"repositories": ["a/b"], "repository_access": "read", "repository_base": "main"}"#,
            Mode::Authoring
        )
        .is_err());
    }

    #[test]
    fn a_write_binding_authored_without_a_base_is_refused() {
        assert!(
            parse(
                r#"{"repositories": ["a/b"], "repository_access": "write"}"#,
                Mode::Authoring
            )
            .is_err()
        );
    }

    #[test]
    fn a_stored_write_binding_without_a_base_survives_for_the_lease_path_to_refuse() {
        let binding = parse(
            r#"{"repositories": ["a/b"], "repository_access": "write"}"#,
            Mode::Stored,
        )
        .expect("a pre-base row must parse so the refusal can name the fleet")
        .expect("a binding was declared");

        assert_eq!(binding.base_branch(), None);
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
            parse(
                &format!(
                    r#"{{"repositories": ["a/b"], "repository_access": "write", "repository_base": "{base}"}}"#
                ),
                Mode::Authoring,
            )
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
