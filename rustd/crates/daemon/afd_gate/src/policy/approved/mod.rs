//! The grants a person approved for a fleet: the one source of what its run
//! may hold.
//!
//! A run is issued exactly the access a person approved when its fleet asked
//! (`afd_approval`'s request and resolve). Pending and revoked grants are
//! absent from [`Approved`] rather than told apart: only `approved` admits
//! anything, and a reader that could see `pending` would be invited to treat
//! it as a maybe.

use af_core::id::Uuid7;
use af_fleet_runtime::config::RepositoryBinding;
use afd_api_wire::grant::status;
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::policy::egress::CREDENTIAL_GITHUB;

/// Statement name, for the context a query failure carries.
const CONTEXT_APPROVED: &str = "approved grants";

/// Every grant a person approved for one fleet.
///
/// `$1` fleet, `$2` the approved status.
pub const SELECT_APPROVED_GRANTS: &str = include_str!("sql/select_approved_grants.sql");

/// One approved grant, held under a share lock through the caller's commit.
///
/// `$1` fleet, `$2` name, `$3` the approved status.
pub const HOLD_APPROVED_GRANT: &str = include_str!("sql/hold_approved_grant.sql");

/// One grant a person approved.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ApprovedGrant {
    /// The vault key or integration the fleet asked for.
    pub name: String,
    /// The channel it reaches a run through, from `afd_api_wire::grant::grant_type`.
    pub grant_type: String,
    /// The approved GitHub scope, as the JSON
    /// `af_fleet_runtime::config::RepositoryBinding::recorded` writes.
    pub scope: Option<String>,
}

/// Every grant a person approved for one fleet, ordered by name.
///
/// No `Default`: a fleet holding nothing is [`Approved::none`], a sentence
/// someone wrote on purpose, so a caller that forgot the read cannot get the
/// empty set by accident and never learn it forgot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approved(Vec<ApprovedGrant>);

impl Approved {
    /// A fleet a person approved nothing for.
    #[must_use]
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    /// The grants a person approved for `fleet`, read on the caller's
    /// connection.
    ///
    /// # Errors
    /// Reports a statement that would not run.
    pub async fn read_on(connection: &mut PgConnection, fleet: &Uuid7) -> Result<Self> {
        sqlx::query_as::<_, ApprovedGrant>(SELECT_APPROVED_GRANTS)
            .bind(fleet)
            .bind(status::APPROVED)
            .fetch_all(connection)
            .await
            .map(Self)
            .map_err(query(CONTEXT_APPROVED))
    }

    /// The approved names, for `afd_credential::secrets::declared` to resolve.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.0.iter().map(|grant| grant.name.as_str()).collect()
    }

    /// Whether `name` was approved as `grant_type`.
    #[must_use]
    pub fn holds(&self, name: &str, grant_type: &str) -> bool {
        self.0
            .iter()
            .any(|grant| grant.name == name && grant.grant_type == grant_type)
    }

    /// The repository binding the approved GitHub scope builds, or `None`
    /// when GitHub is not approved.
    ///
    /// # Errors
    /// A stored scope the parser refuses. The request parsed it before a
    /// person approved it, so this is a row nothing in this daemon writes, and
    /// it fails the run until a person asks for the access again.
    pub fn repository_binding(&self) -> Result<Option<RepositoryBinding>> {
        Ok(self
            .0
            .iter()
            .find(|grant| grant.name == CREDENTIAL_GITHUB)
            .and_then(|grant| grant.scope.as_deref())
            .map(RepositoryBinding::parse)
            .transpose()?)
    }
}

/// Built from grants a test or a caller already holds.
impl FromIterator<ApprovedGrant> for Approved {
    fn from_iter<I: IntoIterator<Item = ApprovedGrant>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Holds `fleet`'s approved grant for `name` through the caller's commit, so a
/// revoke waits for whatever the caller issues under it.
///
/// # Errors
/// Reports a statement that would not run.
pub async fn hold_approved_on(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    name: &str,
) -> Result<bool> {
    sqlx::query_scalar::<_, String>(HOLD_APPROVED_GRANT)
        .bind(fleet)
        .bind(name)
        .bind(status::APPROVED)
        .fetch_optional(connection)
        .await
        .map(|held| held.is_some())
        .map_err(query(CONTEXT_APPROVED))
}

#[cfg(test)]
mod tests;
