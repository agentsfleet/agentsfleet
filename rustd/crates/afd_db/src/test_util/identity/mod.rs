//! The `identity.tenants` and `identity.workspaces` rows a suite seeds, spelled
//! once.
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

const CONTEXT_TENANT: &str = "seed a tenant";
const CONTEXT_WORKSPACE: &str = "seed a workspace";

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
