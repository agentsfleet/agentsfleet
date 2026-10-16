//! Operator reads include staged entries; tenant readers require publication.

use super::{PLATFORM, PLATFORM_DRAFT, project};
use crate::{
    Result, Setup, draft,
    error::{ErrorKind, raise::query},
    library::{CONTEXT, Entry, read},
    types::PlatformLibrary,
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

const LIST: &str = "SELECT to_jsonb(l)::text FROM core.fleet_libraries l WHERE platform_slug IS NOT NULL AND visibility IN ($1,$2) ORDER BY created_at DESC,id DESC";
const LOCK: &str = "SELECT to_jsonb(l)::text FROM core.fleet_libraries l WHERE platform_slug=$1 AND visibility IN ($2,$3) FOR UPDATE";
const RECEIPT: &str = "SELECT l.platform_slug FROM core.fleet_libraries l JOIN core.fleet_library_revisions r ON r.library_id=l.id WHERE r.id=$1::uuid AND l.platform_slug IS NOT NULL";

impl Setup {
    /// Lists staged and published native platform metadata.
    ///
    /// # Errors
    /// Preserves database failures. The operator route requires platform scope.
    pub async fn platform_libraries(&self) -> Result<Vec<PlatformLibrary>> {
        let mut connection = self.drafts.database.acquire().await?;
        let rows: Vec<String> = sqlx::query_scalar(LIST)
            .bind(PLATFORM)
            .bind(PLATFORM_DRAFT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let entry: Entry = draft::value(&row)?;
            let head = read::revision(&mut connection, &entry.id, &entry.head_revision_id).await?;
            items.push(project(&entry, &head));
        }
        Ok(items)
    }
}

pub(in crate::library) async fn lock(connection: &mut PgConnection, slug: &str) -> Result<Entry> {
    read::valid_id(slug)?;
    let row: Option<String> = sqlx::query_scalar(LOCK)
        .bind(slug)
        .bind(PLATFORM)
        .bind(PLATFORM_DRAFT)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    draft::value(&row.ok_or(ErrorKind::Hidden)?)
}

pub(in crate::library) async fn receipt(
    connection: &mut PgConnection,
    revision: &Uuid7,
) -> Result<crate::types::Published> {
    let slug: Option<String> = sqlx::query_scalar(RECEIPT)
        .bind(revision.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let entry = lock(connection, &slug.ok_or(ErrorKind::Gone)?).await?;
    Ok(crate::types::Published {
        library_id: entry.public_id(),
        revision_id: revision.clone(),
        head_revision_id: entry.head_revision_id.clone(),
        etag: entry.etag(),
    })
}
