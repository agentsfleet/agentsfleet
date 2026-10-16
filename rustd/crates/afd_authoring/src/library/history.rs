//! Immutable history is discoverable only through its still-visible entry.

use super::{CONTEXT, read};
use crate::{
    Access, Result, Setup, access, draft,
    error::raise::query,
    paging, replay,
    types::{LibraryQuery, Page, RevisionSummary},
};
use afd_core::id::Uuid7;
use serde::Deserialize;
use sqlx::Connection as _;

const PAGE: &str = "SELECT jsonb_build_object('id',id,'revision',revision,'canonical_digest',canonical_digest,'created_at',created_at)::text
FROM core.fleet_library_revisions WHERE library_id=$1::uuid
AND ($2::bigint IS NULL OR (created_at,id)<($2,$3::uuid)) ORDER BY created_at DESC,id DESC LIMIT $4";

#[derive(Deserialize)]
struct Summary {
    id: Uuid7,
    revision: i64,
    canonical_digest: String,
    created_at: i64,
}

impl Setup {
    /// Lists immutable revision metadata without loading private or instruction text.
    ///
    /// # Errors
    /// Hides deleted/foreign entries and rejects foreign continuations.
    pub async fn library_revisions(
        &self,
        access: Access<'_>,
        library: &str,
        request: LibraryQuery,
    ) -> Result<Page<RevisionSummary>> {
        let request = garde::Unvalidated::new(request).validate()?;
        read::valid_id(library)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = read::entry(&mut tx, &person.owner, library, false).await?;
        let binding = replay::digest(&(
            "native-library-history",
            access.workspace,
            &person.id,
            library,
            request.limit,
        ))?;
        let after = paging::boundary(request.starting_after.as_deref(), &binding)?;
        let rows: Vec<String> = sqlx::query_scalar(PAGE)
            .bind(entry.id.as_str())
            .bind(after.as_ref().map(|(at, _)| *at))
            .bind(after.as_ref().map(|(_, id)| id.as_str()))
            .bind(i64::from(request.limit) + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let more = rows.len() > request.limit as usize;
        let mut items = Vec::new();
        let mut next_cursor = None;
        for row in rows.into_iter().take(request.limit as usize) {
            let row: Summary = draft::value(&row)?;
            next_cursor = Some(paging::cursor(&binding, row.created_at, &row.id));
            items.push(RevisionSummary {
                id: row.id,
                revision: row.revision.to_string(),
                canonical_digest: row.canonical_digest,
                created_at: row.created_at,
            });
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Page {
            items,
            total: None,
            next_cursor: if more { next_cursor } else { None },
        })
    }
}
