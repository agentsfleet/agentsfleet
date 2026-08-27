//! Transactional Postgres repository for platform catalogue edits.

use afd_core::clock::UnixMillis;
use afd_db::Db;
use sqlx::{Acquire as _, Row as _};

use super::etag::{compute, matches_if_match};
use super::{DeleteLibrary, LibraryItem, LibraryPatch, LibraryRequirements, PatchLibrary};
use crate::{Error, Result};

const CONTEXT_LIST: &str = "list platform library";
const CONTEXT_PATCH: &str = "patch platform library";
const CONTEXT_DELETE: &str = "delete platform library";

/// Platform Fleet-library repository.
#[derive(Debug, Clone)]
pub struct Libraries(pub(super) Db);

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
        sqlx::query(LIST)
            .fetch_all(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_LIST))?
            .iter()
            .map(|row_value| decode(row_value, CONTEXT_LIST))
            .collect()
    }

    /// Applies a row-locked, optionally versioned partial edit.
    ///
    /// Changing source identity atomically discards its stale content hash and
    /// returns the row to draft.
    ///
    /// # Errors
    /// Reports transaction or malformed-row failures.
    pub async fn patch(
        &self,
        id: &str,
        patch: &LibraryPatch,
        if_match: Option<&str>,
        now: UnixMillis,
    ) -> Result<PatchLibrary> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(Error::database(CONTEXT_PATCH))?;
        let Some(current_row) = sqlx::query(SELECT_FOR_UPDATE)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(Error::database(CONTEXT_PATCH))?
        else {
            return Ok(PatchLibrary::NotFound);
        };
        let current = decode(&current_row, CONTEXT_PATCH)?;
        if if_match.is_some_and(|tag| !matches_if_match(tag, current.etag())) {
            return Ok(PatchLibrary::Stale {
                etag: current.etag().to_owned(),
            });
        }
        let source_changed = patch
            .source_repo
            .as_deref()
            .is_some_and(|source| source != current.source_repo())
            || patch
                .source_ref
                .as_deref()
                .is_some_and(|source| source != current.source_ref());
        if patch.published == Some(true) && (source_changed || current.content_hash().is_none()) {
            return Ok(PatchLibrary::PublishWithoutBundle);
        }
        let reasons = patch
            .required_credentials_reasons
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let row_value = sqlx::query(UPDATE)
            .bind(id)
            .bind(&patch.name)
            .bind(&patch.description)
            .bind(&patch.source_repo)
            .bind(&patch.source_ref)
            .bind(reasons)
            .bind(patch.published)
            .bind(now.as_millis())
            .fetch_one(&mut *transaction)
            .await
            .map_err(Error::database(CONTEXT_PATCH))?;
        let updated = decode(&row_value, CONTEXT_PATCH)?;
        transaction
            .commit()
            .await
            .map_err(Error::database(CONTEXT_PATCH))?;
        Ok(PatchLibrary::Updated(Box::new(updated)))
    }

    /// Deletes only a non-public row in one guarded statement.
    ///
    /// # Errors
    /// Reports a database failure.
    pub async fn delete(&self, id: &str) -> Result<DeleteLibrary> {
        let mut connection = self.0.acquire().await?;
        let outcome: String = sqlx::query_scalar(DELETE)
            .bind(id)
            .fetch_one(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_DELETE))?;
        Ok(match outcome.as_str() {
            "deleted" => DeleteLibrary::Deleted,
            "published" => DeleteLibrary::Published,
            _ => DeleteLibrary::NotFound,
        })
    }
}

fn decode(row: &sqlx::postgres::PgRow, context: &'static str) -> Result<LibraryItem> {
    let id = row.try_get(0).map_err(Error::database(context))?;
    let name: String = row.try_get(1).map_err(Error::database(context))?;
    let description: String = row.try_get(2).map_err(Error::database(context))?;
    let source_repo: String = row.try_get(3).map_err(Error::database(context))?;
    let source_ref: String = row.try_get(4).map_err(Error::database(context))?;
    let visibility: String = row.try_get(5).map_err(Error::database(context))?;
    let content_hash = row.try_get(6).map_err(Error::database(context))?;
    let credentials_raw: String = row.try_get(7).map_err(Error::database(context))?;
    let tools_raw: String = row.try_get(8).map_err(Error::database(context))?;
    let hosts_raw: String = row.try_get(9).map_err(Error::database(context))?;
    let reasons_raw: String = row.try_get(10).map_err(Error::database(context))?;
    let trigger_present = row.try_get(11).map_err(Error::database(context))?;
    let updated_at = UnixMillis::from_millis(row.try_get(12).map_err(Error::database(context))?);
    let etag = compute(&[
        Some(&name),
        Some(&description),
        Some(&source_repo),
        Some(&source_ref),
        Some(&reasons_raw),
        Some(&visibility),
    ]);
    Ok(LibraryItem {
        id,
        name,
        description,
        source_repo,
        source_ref,
        visibility,
        content_hash,
        requirements: LibraryRequirements::new(
            serde_json::from_str(&credentials_raw)?,
            serde_json::from_str(&tools_raw)?,
            serde_json::from_str(&hosts_raw)?,
            trigger_present,
        ),
        required_credentials_reasons: serde_json::from_str(&reasons_raw)?,
        updated_at,
        etag,
    })
}

#[cfg(test)]
const PROJECTION: &str = "id,name,description,source_repo,source_ref,visibility,content_hash,required_credentials::text,required_tools::text,network_hosts::text,required_credentials_reasons::text,(trigger_markdown IS NOT NULL),updated_at";
const LIST: &str = "SELECT id,name,description,source_repo,source_ref,visibility,content_hash,required_credentials::text,required_tools::text,network_hosts::text,required_credentials_reasons::text,(trigger_markdown IS NOT NULL),updated_at FROM core.fleet_library ORDER BY id";
const SELECT_FOR_UPDATE: &str = "SELECT id,name,description,source_repo,source_ref,visibility,content_hash,required_credentials::text,required_tools::text,network_hosts::text,required_credentials_reasons::text,(trigger_markdown IS NOT NULL),updated_at FROM core.fleet_library WHERE id=$1 FOR UPDATE";
const UPDATE: &str = "UPDATE core.fleet_library SET name=COALESCE($2,name),description=COALESCE($3,description),content_hash=CASE WHEN COALESCE($4,source_repo) IS DISTINCT FROM source_repo OR COALESCE($5,source_ref) IS DISTINCT FROM source_ref THEN NULL ELSE content_hash END,visibility=CASE WHEN COALESCE($4,source_repo) IS DISTINCT FROM source_repo OR COALESCE($5,source_ref) IS DISTINCT FROM source_ref THEN 'draft' WHEN $7::boolean IS TRUE THEN 'public' WHEN $7::boolean IS FALSE THEN 'draft' ELSE visibility END,source_repo=COALESCE($4,source_repo),source_ref=COALESCE($5,source_ref),required_credentials_reasons=COALESCE($6::jsonb,required_credentials_reasons),updated_at=$8 WHERE id=$1 RETURNING id,name,description,source_repo,source_ref,visibility,content_hash,required_credentials::text,required_tools::text,network_hosts::text,required_credentials_reasons::text,(trigger_markdown IS NOT NULL),updated_at";
const DELETE: &str = "WITH target AS (SELECT visibility FROM core.fleet_library WHERE id=$1), removed AS (DELETE FROM core.fleet_library WHERE id=$1 AND visibility <> 'public' RETURNING 1) SELECT CASE WHEN EXISTS(SELECT 1 FROM removed) THEN 'deleted' WHEN EXISTS(SELECT 1 FROM target WHERE visibility='public') THEN 'published' ELSE 'not_found' END";

#[cfg(test)]
mod tests {
    use super::{LIST, PROJECTION, SELECT_FOR_UPDATE, UPDATE};

    #[test]
    fn every_catalogue_read_is_metadata_only_and_projection_aligned() {
        for statement in [LIST, SELECT_FOR_UPDATE, UPDATE] {
            assert!(!statement.contains("skill_markdown,"));
            assert!(!statement.contains("support_files_json"));
            assert!(!statement.contains("snapshot_key"));
        }
        assert!(LIST.contains(PROJECTION));
        assert!(SELECT_FOR_UPDATE.contains(PROJECTION));
        assert!(UPDATE.contains(PROJECTION));
    }
}
