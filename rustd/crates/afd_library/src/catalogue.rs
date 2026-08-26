//! Metadata-only platform library CRUD.

use afd_core::clock::UnixMillis;
use afd_db::Db;
use serde_json::Value;
use sqlx::Row as _;

use crate::{Error, Result};

const CONTEXT_LIST: &str = "list platform library";
const CONTEXT_PATCH: &str = "patch platform library";
const CONTEXT_DELETE: &str = "delete platform library";

/// One admin catalogue row; bundle bodies and object keys are not projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryItem {
    /// Slug identity from `SKILL.md`.
    pub id: String,
    /// Operator-visible name.
    pub name: String,
    /// Operator-curated description.
    pub description: String,
    /// GitHub owner/repository.
    pub source_repo: String,
    /// Git branch, tag, or commit fetched.
    pub source_ref: String,
    /// Draft or public.
    pub visibility: String,
    /// Immutable bundle identity when fetched.
    pub content_hash: Option<String>,
    /// Required credential names only.
    pub required_credentials: Value,
    /// Required tool identifiers.
    pub required_tools: Value,
    /// Declared outbound hosts.
    pub network_hosts: Value,
    /// Whether the bundle has a trigger document.
    pub trigger_present: bool,
    /// Last mutation instant.
    pub updated_at: UnixMillis,
}

/// Curated fields accepted by an admin patch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryPatch {
    /// Replacement display name.
    pub name: Option<String>,
    /// Replacement description.
    pub description: Option<String>,
    /// Replacement source repository; invalidates a stored bundle.
    pub source_repo: Option<String>,
    /// Replacement source revision; invalidates a stored bundle.
    pub source_ref: Option<String>,
    /// Draft/public transition.
    pub visibility: Option<String>,
}

/// Platform library repository.
#[derive(Debug, Clone)]
pub struct Libraries(Db);

impl Libraries {
    /// Uses the already-connected API-role pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self(database)
    }

    /// Lists metadata without root-document or support-file bytes.
    ///
    /// # Errors
    /// Reports a database or malformed-JSON failure.
    pub async fn list(&self) -> Result<Vec<LibraryItem>> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(LIST).fetch_all(&mut *connection).await
            .map_err(Error::database(CONTEXT_LIST))?.iter().map(row).collect()
    }

    /// Applies curated fields and returns whether a row changed.
    ///
    /// Changing source identity atomically discards its stale content hash and
    /// returns the row to draft.
    ///
    /// # Errors
    /// Reports a database failure.
    pub async fn patch(&self, id: &str, patch: &LibraryPatch, now: UnixMillis) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(PATCH).bind(id).bind(&patch.name).bind(&patch.description)
            .bind(&patch.source_repo).bind(&patch.source_ref).bind(&patch.visibility)
            .bind(now.as_millis()).execute(&mut *connection).await
            .map(|done| done.rows_affected() != 0).map_err(Error::database(CONTEXT_PATCH))
    }

    /// Deletes only a non-public row.
    ///
    /// # Errors
    /// Reports a database failure.
    pub async fn delete_draft(&self, id: &str) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(DELETE).bind(id).execute(&mut *connection).await
            .map(|done| done.rows_affected() != 0).map_err(Error::database(CONTEXT_DELETE))
    }
}

fn row(row: &sqlx::postgres::PgRow) -> Result<LibraryItem> {
    let json = |index| -> Result<Value> {
        let raw: String = row.try_get(index).map_err(Error::database(CONTEXT_LIST))?;
        serde_json::from_str(&raw).map_err(Error::from)
    };
    Ok(LibraryItem {
        id: row.try_get(0).map_err(Error::database(CONTEXT_LIST))?,
        name: row.try_get(1).map_err(Error::database(CONTEXT_LIST))?,
        description: row.try_get(2).map_err(Error::database(CONTEXT_LIST))?,
        source_repo: row.try_get(3).map_err(Error::database(CONTEXT_LIST))?,
        source_ref: row.try_get(4).map_err(Error::database(CONTEXT_LIST))?,
        visibility: row.try_get(5).map_err(Error::database(CONTEXT_LIST))?,
        content_hash: row.try_get(6).map_err(Error::database(CONTEXT_LIST))?,
        required_credentials: json(7)?,
        required_tools: json(8)?,
        network_hosts: json(9)?,
        trigger_present: row.try_get(10).map_err(Error::database(CONTEXT_LIST))?,
        updated_at: UnixMillis::from_millis(row.try_get(11).map_err(Error::database(CONTEXT_LIST))?),
    })
}

const LIST: &str = "SELECT id,name,description,source_repo,source_ref,visibility,content_hash,required_credentials::text,required_tools::text,network_hosts::text,(trigger_markdown IS NOT NULL),updated_at FROM core.fleet_library ORDER BY id";
const PATCH: &str = "UPDATE core.fleet_library SET name=COALESCE($2,name),description=COALESCE($3,description),content_hash=CASE WHEN COALESCE($4,source_repo) IS DISTINCT FROM source_repo OR COALESCE($5,source_ref) IS DISTINCT FROM source_ref THEN NULL ELSE content_hash END,visibility=CASE WHEN COALESCE($4,source_repo) IS DISTINCT FROM source_repo OR COALESCE($5,source_ref) IS DISTINCT FROM source_ref THEN 'draft' ELSE COALESCE($6,visibility) END,source_repo=COALESCE($4,source_repo),source_ref=COALESCE($5,source_ref),updated_at=$7 WHERE id=$1 AND ($6 IS NULL OR $6 <> 'public' OR content_hash IS NOT NULL)";
const DELETE: &str = "DELETE FROM core.fleet_library WHERE id=$1 AND visibility <> 'public'";
