//! The tenant's workspace directory: the list, and the create beside it.
//!
//! # Why the list is two statements where the Zig one is one
//!
//! `tenant_workspaces.zig` folds the tenant resolve into the page select with
//! a lateral join and a marker row, so a tenant with zero workspaces still
//! answers its own identifier. Here the resolve is [`super::Workspaces::
//! tenant_of`] — the ONE statement every tenant route shares — and the page
//! select takes the resolved tenant as a bind. A second spelling of the
//! authority order to save one round trip would be two places for that order
//! to drift apart, and the wire answer is byte-identical either way.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use sqlx::Row as _;

use crate::sql::workspace as sql;
use crate::{Result, error};

use super::Workspaces;
use super::name::{self, Chosen};

/// The context a datastore failure on the page walk reports under.
const CONTEXT_PAGE: &str = "list tenant workspaces";

/// The context a row this daemon cannot read reports under.
const CONTEXT_ROW: &str = "read workspace row";

/// The context the create's insert reports under.
const CONTEXT_CREATE: &str = "create workspace";

/// The context the tenant-existence read reports under.
const CONTEXT_TENANT: &str = "check tenant exists";

/// Postgres's unique-violation SQLSTATE.
const UNIQUE_VIOLATION: &str = "23505";

/// The index that arbitrates a per-tenant name.
///
/// Must equal the name in `schema/210_workspaces.sql`, because classification
/// is by exact constraint: a rename landing on one side turns a duplicate-name
/// conflict into a 500 — the regression `lifecycle.zig`'s comment records.
const NAME_CONSTRAINT: &str = "uq_workspaces_tenant_id_name";

/// How many generated names the create tries before reporting the collision.
///
/// The suffix makes a collision roughly one in a million per attempt, so a
/// third loss in a row is evidence of something broken — an entropy source
/// gone flat — and worth surfacing rather than retrying forever.
const GENERATED_ATTEMPTS: u32 = 3;

/// One workspace as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    /// The workspace's identifier.
    pub id: String,
    /// Its name — `NULL` on rows older than naming, and serialized as null.
    pub name: Option<String>,
    /// When it was created; the walk's sort key.
    pub created_at_ms: i64,
}

/// One page of the walk, and whether a row exists beyond it.
///
/// `more` is decided by fetching one row past the limit, the way the Zig
/// handler does — so the cursor is emitted only when a next page actually has
/// something on it, not merely when this one is full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePage {
    /// The rows on this page, oldest first.
    pub rows: Vec<WorkspaceRow>,
    /// Whether the walk continues past the last row here.
    pub more: bool,
}

/// A decoded `starting_after` boundary: the last row the previous page showed.
///
/// The identifier is a parsed [`Uuid7`], not a string — the handler refuses a
/// malformed one before a connection is drawn, which is `isSupportedWorkspaceId`
/// as a type rather than a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct After {
    /// The boundary row's `created_at`.
    pub created_at_ms: i64,
    /// The boundary row's identifier, breaking ties within one instant.
    pub id: Uuid7,
}

/// What a create answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// The new workspace's identifier.
    pub id: Uuid7,
    /// Its name — chosen or generated, as stored.
    pub name: String,
}

impl Workspaces {
    /// One page of `tenant`'s workspaces, oldest first.
    ///
    /// `filter` holds the walk to an exact name; `after` is the decoded
    /// cursor when the caller is resuming.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon
    /// cannot read.
    pub async fn page(
        &self,
        tenant: &Uuid7,
        filter: Option<&str>,
        after: Option<&After>,
        limit: u32,
    ) -> Result<WorkspacePage> {
        // One past the limit, so `more` is a fact about the walk rather than
        // a guess about a full page.
        let fetch = i64::from(limit).saturating_add(1);
        let mut connection = self.database.acquire().await?;
        let query = match (filter, after) {
            (None, None) => sqlx::query(sql::SELECT_TENANT_WORKSPACES_PAGE_FIRST)
                .bind(tenant.as_str())
                .bind(fetch),
            (None, Some(boundary)) => sqlx::query(sql::SELECT_TENANT_WORKSPACES_PAGE_AFTER)
                .bind(tenant.as_str())
                .bind(boundary.created_at_ms)
                .bind(boundary.id.as_str())
                .bind(fetch),
            (Some(name), None) => sqlx::query(sql::SELECT_TENANT_WORKSPACES_PAGE_FIRST_BY_NAME)
                .bind(tenant.as_str())
                .bind(name)
                .bind(fetch),
            (Some(name), Some(boundary)) => {
                sqlx::query(sql::SELECT_TENANT_WORKSPACES_PAGE_AFTER_BY_NAME)
                    .bind(tenant.as_str())
                    .bind(name)
                    .bind(boundary.created_at_ms)
                    .bind(boundary.id.as_str())
                    .bind(fetch)
            }
        };
        let fetched = query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_PAGE))?;

        let mut rows = fetched
            .iter()
            .map(read_row)
            .collect::<Result<Vec<WorkspaceRow>>>()?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        Ok(WorkspacePage { rows, more })
    }

    /// Creates one workspace, naming it when the caller did not.
    ///
    /// # Errors
    /// Refuses a session whose tenant has no row behind it, and a chosen name
    /// this tenant already uses. A GENERATED name that collides is retried
    /// with a fresh draw instead — the caller never chose it, so "taken" would
    /// name a conflict they cannot see. Also reports a host that cannot draw
    /// entropy and a datastore that would not answer.
    pub async fn create(
        &self,
        tenant: &Uuid7,
        chosen: Option<Chosen>,
        created_by: &str,
        now: UnixMillis,
    ) -> Result<Created> {
        let mut connection = self.database.acquire().await?;
        // Asked before the insert for `lifecycle.zig`'s reason: a stale
        // session can name a deleted tenant, and this sentence beats the
        // foreign key's 500. The race between check and insert stays — Zig
        // has the same one — and loses only a clearer refusal.
        let exists: Option<(i32,)> = sqlx::query_as(sql::SELECT_TENANT_EXISTS)
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_TENANT))?;
        if exists.is_none() {
            return Err(error::workspace_tenant_vanished());
        }

        if let Some(name) = chosen {
            let id = self.mint_id(now)?;
            return match self
                .insert(&mut connection, &id, tenant, name.as_str(), created_by, now)
                .await
            {
                Ok(()) => Ok(Created {
                    id,
                    name: name.as_str().to_owned(),
                }),
                Err(source) if is_name_conflict(&source) => Err(error::workspace_name_exists()),
                Err(source) => Err(error::query(CONTEXT_CREATE)(source)),
            };
        }

        let mut attempt = 0;
        loop {
            let name = name::generate(&self.entropy)?;
            let id = self.mint_id(now)?;
            match self
                .insert(&mut connection, &id, tenant, &name, created_by, now)
                .await
            {
                Ok(()) => return Ok(Created { id, name }),
                Err(source) if is_name_conflict(&source) && attempt + 1 < GENERATED_ATTEMPTS => {
                    attempt += 1;
                }
                // Exhausted, or broken some other way. Either way the chain
                // is the log's; the caller hears the datastore refused.
                Err(source) => return Err(error::query(CONTEXT_CREATE)(source)),
            }
        }
    }

    /// One insert, answering the driver's own error for the caller to classify.
    async fn insert(
        &self,
        connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        id: &Uuid7,
        tenant: &Uuid7,
        name: &str,
        created_by: &str,
        now: UnixMillis,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(sql::INSERT_WORKSPACE)
            .bind(id.as_str())
            .bind(tenant.as_str())
            .bind(name)
            .bind(created_by)
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map(|_outcome| ())
    }

    /// Draws a fresh workspace identifier.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

/// Reads one row by column name, through [`error::query`] with one context —
/// a `try_get` failure already names the column and the type it refused.
fn read_row(row: &sqlx::postgres::PgRow) -> Result<WorkspaceRow> {
    let unreadable = error::query(CONTEXT_ROW);
    Ok(WorkspaceRow {
        id: row.try_get("id").map_err(&unreadable)?,
        name: row.try_get("name").map_err(&unreadable)?,
        created_at_ms: row.try_get("created_at").map_err(&unreadable)?,
    })
}

/// Tells a lost name race apart from a broken statement.
///
/// By exact constraint, not by SQLSTATE alone: the table carries a second
/// unique constraint on `(id, tenant_id)`, and an identifier collision — one
/// entropy draw repeating another to the bit — is not a fact about the NAME.
fn is_name_conflict(source: &sqlx::Error) -> bool {
    source.as_database_error().is_some_and(|failure| {
        failure.code().is_some_and(|code| code == UNIQUE_VIOLATION)
            && failure.constraint() == Some(NAME_CONSTRAINT)
    })
}
