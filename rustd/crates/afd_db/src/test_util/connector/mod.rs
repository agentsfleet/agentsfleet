//! The `connector.integration_grants` and `connector.installs` rows a suite
//! seeds, spelled once, the way [`super::FleetSeed`] seeds a fleet.
//!
//! A grant is what lets a fleet reach a service, and an install is what maps an
//! outside account to a workspace. The ingress, mint and callback suites each
//! need one before they can reach the path they prove.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a grant seed sets.
const INSERT_GRANT: &str = include_str!("sql/insert_grant.sql");

/// Every column an install seed sets.
const INSERT_INSTALL: &str = include_str!("sql/insert_install.sql");

const CONTEXT_GRANT: &str = "seed an integration grant";
const CONTEXT_INSTALL: &str = "seed a connector install";

/// The reason a seeded grant was requested for, when the suite names none.
const FIXTURE_REASON: &str = "fixture";

/// One fleet's grant over one service, in `status`. Neither approved nor
/// revoked, and stamped by the database's `now()`, unless the suite says so.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct GrantSeed<'a> {
    fleet_id: &'a str,
    service: &'a str,
    status: &'a str,
    approved_at: Option<Timestamp>,
    revoked_at: Option<Timestamp>,
    created_at: Option<Timestamp>,
}

impl<'a> GrantSeed<'a> {
    /// A grant of `service` to `fleet_id`, in `status`.
    pub const fn new(fleet_id: &'a str, service: &'a str, status: &'a str) -> Self {
        Self {
            fleet_id,
            service,
            status,
            approved_at: None,
            revoked_at: None,
            created_at: None,
        }
    }

    /// When a person approved it.
    pub const fn approved(mut self, instant: Timestamp) -> Self {
        self.approved_at = Some(instant);
        self
    }

    /// When it was revoked.
    pub const fn revoked(mut self, instant: Timestamp) -> Self {
        self.revoked_at = Some(instant);
        self
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row, answering the grant's minted id.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing fleet, or a second
    /// grant of the same service.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<String> {
        let id = super::mint_id();
        sqlx::query(INSERT_GRANT)
            .bind(&id)
            .bind(self.fleet_id)
            .bind(self.service)
            .bind(self.status)
            .bind(FIXTURE_REASON)
            .bind(self.approved_at.map(Timestamp::to_sqlx))
            .bind(self.revoked_at.map(Timestamp::to_sqlx))
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_GRANT, source))?;
        Ok(id)
    }
}

/// One outside account's install, mapped to a workspace. No scopes, and
/// stamped by the database's `now()`, unless the suite says so.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct InstallSeed<'a> {
    provider: &'a str,
    external_account_id: &'a str,
    workspace_id: &'a str,
    scopes: &'a [&'a str],
    created_at: Option<Timestamp>,
}

impl<'a> InstallSeed<'a> {
    /// `provider`'s account `external_account_id`, routed to `workspace_id`.
    pub const fn new(
        provider: &'a str,
        external_account_id: &'a str,
        workspace_id: &'a str,
    ) -> Self {
        Self {
            provider,
            external_account_id,
            workspace_id,
            scopes: &[],
            created_at: None,
        }
    }

    /// The scopes the install was granted.
    pub const fn scopes(mut self, scopes: &'a [&'a str]) -> Self {
        self.scopes = scopes;
        self
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row, answering the install's minted id.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing workspace, or the
    /// account already installed.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<String> {
        let id = super::mint_id();
        sqlx::query(INSERT_INSTALL)
            .bind(&id)
            .bind(self.provider)
            .bind(self.external_account_id)
            .bind(self.workspace_id)
            .bind(self.scopes)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_INSTALL, source))?;
        Ok(id)
    }
}
