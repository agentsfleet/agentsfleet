//! Gallery and owned walks use the same tenant authority with distinct cursors.

use super::{CONTEXT, EntryRow, PLATFORM, Revision, RevisionRow, sql};
use crate::{
    Access, Result, Setup, access,
    error::raise::query,
    paging, replay,
    types::{LibraryCard, LibraryQuery, Page},
};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

/// [`sql::PAGE`]'s row: an entry and its head revision, by column name.
#[derive(sqlx::FromRow)]
struct Listed {
    #[sqlx(flatten)]
    entry: EntryRow,
    head_revision: i64,
    head_definition: String,
    head_canonical_digest: String,
    head_policy_semantics: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    head_created_at: Timestamp,
}

impl Listed {
    fn card(self) -> Result<(Timestamp, afd_core::id::Uuid7, LibraryCard)> {
        let entry = self.entry.decode()?;
        let head: Revision = RevisionRow {
            id: entry.head_revision_id.to_string(),
            revision: self.head_revision,
            definition: self.head_definition,
            canonical_digest: self.head_canonical_digest,
            policy_semantics: self.head_policy_semantics,
            created_at: self.head_created_at,
        }
        .decode()?;
        Ok((entry.created_at, entry.id.clone(), entry.card(&head)))
    }
}

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
        let rows: Vec<Listed> = sqlx::query_as(sql::PAGE)
            .bind(person.owner.as_str())
            .bind(owned)
            .bind(PLATFORM)
            .bind(after.as_ref().map(|(at, _)| at.to_sqlx()))
            .bind(after.as_ref().map(|(_, id)| id.as_str()))
            .bind(i64::from(request.limit) + 1)
            .bind(super::TENANT)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        let more = rows.len() > request.limit as usize;
        let mut items = Vec::new();
        let mut next_cursor = None;
        for row in rows.into_iter().take(request.limit as usize) {
            let (at, id, card) = row.card()?;
            next_cursor = Some(paging::cursor(&binding, at, &id));
            items.push(card);
        }
        Ok(Page {
            items,
            total: None,
            next_cursor: if more { next_cursor } else { None },
        })
    }
}
