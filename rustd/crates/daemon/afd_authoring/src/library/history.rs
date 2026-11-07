//! Immutable history is discoverable only through its still-visible entry.

use super::{CONTEXT, read, sql};
use crate::{
    Access, Result, Setup, access,
    error::raise::query,
    paging, replay,
    types::{LibraryQuery, Page, RevisionSummary},
};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

/// [`sql::HISTORY`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Summary {
    id: String,
    revision: i64,
    canonical_digest: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
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
        let rows: Vec<Summary> = sqlx::query_as(sql::HISTORY)
            .bind(entry.id.as_str())
            .bind(after.as_ref().map(|(at, _)| at.to_sqlx()))
            .bind(after.as_ref().map(|(_, id)| id.as_str()))
            .bind(i64::from(request.limit) + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let more = rows.len() > request.limit as usize;
        let mut items = Vec::new();
        let mut next_cursor = None;
        for row in rows.into_iter().take(request.limit as usize) {
            let id = Uuid7::parse(&row.id)?;
            next_cursor = Some(paging::cursor(&binding, row.created_at, &id));
            items.push(RevisionSummary {
                id,
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
