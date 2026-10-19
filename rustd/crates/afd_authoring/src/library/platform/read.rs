//! Operator reads include staged entries; tenant readers require publication.

use super::{PLATFORM, PLATFORM_DRAFT, project};
use crate::{
    Result, Setup,
    error::{ErrorKind, raise::query},
    library::{CONTEXT, Entry, EntryRow, read, sql},
    types::PlatformLibrary,
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

impl Setup {
    /// Lists staged and published native platform metadata.
    ///
    /// # Errors
    /// Preserves database failures. The operator route requires platform scope.
    pub async fn platform_libraries(&self) -> Result<Vec<PlatformLibrary>> {
        let mut connection = self.drafts.database.acquire().await?;
        let rows: Vec<EntryRow> = sqlx::query_as(sql::PLATFORM_LIST)
            .bind(PLATFORM)
            .bind(PLATFORM_DRAFT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = row.decode()?;
            let head = read::revision(&mut connection, &entry.id, &entry.head_revision_id).await?;
            items.push(project(&entry, &head));
        }
        Ok(items)
    }
}

pub(in crate::library) async fn lock(connection: &mut PgConnection, slug: &str) -> Result<Entry> {
    read::valid_id(slug)?;
    let row: Option<EntryRow> = sqlx::query_as(sql::PLATFORM_LOCK)
        .bind(slug)
        .bind(PLATFORM)
        .bind(PLATFORM_DRAFT)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::Hidden)?.decode()
}

pub(in crate::library) async fn receipt(
    connection: &mut PgConnection,
    revision: &Uuid7,
) -> Result<crate::types::Published> {
    let slug: Option<String> = sqlx::query_scalar(sql::PLATFORM_RECEIPT)
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
