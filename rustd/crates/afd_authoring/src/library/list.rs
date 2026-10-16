//! Gallery and owned walks use the same tenant authority with distinct cursors.

use super::{CONTEXT, Entry, PLATFORM, read};
use crate::{
    Access, Result, Setup, access, draft,
    error::raise::query,
    paging, replay,
    types::{LibraryCard, LibraryQuery, Page},
};
use sqlx::Connection as _;

const PAGE: &str = "SELECT to_jsonb(l)::text FROM core.fleet_libraries l WHERE
(l.tenant_id=$1::uuid AND (NOT $2 OR l.visibility=$7) OR (NOT $2 AND l.visibility=$3))
AND ($4::bigint IS NULL OR (l.created_at,l.id)<($4,$5::uuid))
ORDER BY l.created_at DESC,l.id DESC LIMIT $6";

impl Setup {
    /// Lists native Library cards visible to this workspace; owned excludes platform.
    ///
    /// # Errors
    /// Rejects inaccessible workspaces, invalid bounds and foreign continuations.
    pub async fn libraries(
        &self,
        access: Access<'_>,
        request: LibraryQuery,
        owned: bool,
    ) -> Result<Page<LibraryCard>> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let binding = replay::digest(&(
            "native-library",
            access.workspace,
            &person.id,
            owned,
            request.limit,
        ))?;
        let after = paging::boundary(request.starting_after.as_deref(), &binding)?;
        let rows: Vec<String> = sqlx::query_scalar(PAGE)
            .bind(person.owner.as_str())
            .bind(owned)
            .bind(PLATFORM)
            .bind(after.as_ref().map(|(at, _)| *at))
            .bind(after.as_ref().map(|(_, id)| id.as_str()))
            .bind(i64::from(request.limit) + 1)
            .bind(super::TENANT)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let more = rows.len() > request.limit as usize;
        let mut items = Vec::new();
        let mut next_cursor = None;
        for row in rows.into_iter().take(request.limit as usize) {
            let entry: Entry = draft::value(&row)?;
            let head = read::revision(&mut tx, &entry.id, &entry.head_revision_id).await?;
            next_cursor = Some(paging::cursor(&binding, entry.created_at, &entry.id));
            items.push(entry.card(&head));
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Page {
            items,
            total: None,
            next_cursor: if more { next_cursor } else { None },
        })
    }
}
