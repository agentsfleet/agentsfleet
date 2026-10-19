//! The `identity.tenants`, `identity.workspaces`, `identity.users` and
//! `identity.api_keys` rows a suite seeds, spelled once.
//!
//! Nearly every integration suite needs a tenant and a workspace to hang its
//! rows from, and each used to write its own `INSERT` with the column list
//! inline, so a column change meant editing every suite. They build a
//! [`TenantSeed`] and a [`WorkspaceSeed`] instead, the way [`super::FleetSeed`]
//! seeds a fleet, and the column lists change here alone.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a tenant seed sets.
const INSERT_TENANT: &str = include_str!("sql/insert_tenant.sql");

/// Every column a workspace seed sets.
const INSERT_WORKSPACE: &str = include_str!("sql/insert_workspace.sql");

/// Every column an API key seed sets.
const INSERT_API_KEY: &str = include_str!("sql/insert_api_key.sql");

/// Every column a person seed sets.
const INSERT_USER: &str = include_str!("sql/insert_user.sql");

const CONTEXT_TENANT: &str = "seed a tenant";
const CONTEXT_WORKSPACE: &str = "seed a workspace";
const CONTEXT_API_KEY: &str = "seed an API key";
const CONTEXT_USER: &str = "seed a person";

/// One tenant row. Named after its own id unless the suite names it, and
/// stamped by the database's `now()` unless the suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct TenantSeed<'a> {
    id: &'a str,
    name: &'a str,
    created_at: Option<Timestamp>,
}

impl<'a> TenantSeed<'a> {
    /// A tenant named after its own id.
    pub const fn new(id: &'a str) -> Self {
        Self {
            id,
            name: id,
            created_at: None,
        }
    }

    /// Names the tenant, for a suite that asserts on the name.
    pub const fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a duplicate id among them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_TENANT)
            .bind(self.id)
            .bind(self.name)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_TENANT, source))?;
        Ok(())
    }
}

/// One workspace row under a tenant. Named after its own id unless the suite
/// names it, and stamped by the database's `now()` unless the suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct WorkspaceSeed<'a> {
    id: &'a str,
    tenant_id: &'a str,
    name: &'a str,
    created_at: Option<Timestamp>,
}

impl<'a> WorkspaceSeed<'a> {
    /// A workspace named after its own id, under `tenant_id`.
    pub const fn new(id: &'a str, tenant_id: &'a str) -> Self {
        Self {
            id,
            tenant_id,
            name: id,
            created_at: None,
        }
    }

    /// Names the workspace, for a suite that asserts on the name.
    pub const fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// Stamps `created_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing tenant or a duplicate
    /// id.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_WORKSPACE)
            .bind(self.id)
            .bind(self.tenant_id)
            .bind(self.name)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_WORKSPACE, source))?;
        Ok(())
    }
}

/// One live API key under a tenant, stored as the digest of the credential a
/// suite's requests present.
///
/// Named `fixture` unless the suite names it, and stamped by the database's
/// `now()` unless the suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct ApiKeySeed<'a> {
    id: &'a str,
    tenant_id: &'a str,
    name: &'a str,
    key_hash: &'a str,
    created_by_subject: &'a str,
    created_at: Option<Timestamp>,
}

impl<'a> ApiKeySeed<'a> {
    /// A key `id` under `tenant_id`, stored as `key_hash` and made by `subject`.
    pub const fn new(id: &'a str, tenant_id: &'a str, key_hash: &'a str, subject: &'a str) -> Self {
        Self {
            id,
            tenant_id,
            name: "fixture",
            key_hash,
            created_by_subject: subject,
            created_at: None,
        }
    }

    /// Names the key, for a suite that seeds two under one tenant.
    pub const fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing tenant, a duplicate id
    /// or a duplicate digest.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_API_KEY)
            .bind(self.id)
            .bind(self.tenant_id)
            .bind(self.name)
            .bind(self.key_hash)
            .bind(self.created_by_subject)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_API_KEY, source))?;
        Ok(())
    }
}

/// One person under a tenant, signed in as `oidc_subject`.
///
/// No display name unless the suite names one, and stamped by the database's
/// `now()` unless the suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct UserSeed<'a> {
    id: &'a str,
    tenant_id: &'a str,
    oidc_subject: &'a str,
    email: &'a str,
    display_name: Option<&'a str>,
    created_at: Option<Timestamp>,
}

impl<'a> UserSeed<'a> {
    /// A person `id` under `tenant_id`, signed in as `oidc_subject`.
    pub const fn new(
        id: &'a str,
        tenant_id: &'a str,
        oidc_subject: &'a str,
        email: &'a str,
    ) -> Self {
        Self {
            id,
            tenant_id,
            oidc_subject,
            email,
            display_name: None,
            created_at: None,
        }
    }

    /// Names the person, for a suite that asserts on the name.
    pub const fn display_name(mut self, name: &'a str) -> Self {
        self.display_name = Some(name);
        self
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing tenant or a duplicate
    /// id.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_USER)
            .bind(self.id)
            .bind(self.tenant_id)
            .bind(self.oidc_subject)
            .bind(self.email)
            .bind(self.display_name)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_USER, source))?;
        Ok(())
    }
}
