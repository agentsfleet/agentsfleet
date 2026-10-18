//! Creator-only keyset listing binds every filter and the page size.

use crate::{
    Access, Drafts, Result, access,
    error::raise::query,
    paging, replay, sql,
    types::{DraftCard, DraftState, ListQuery, Page},
};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

const CONTEXT: &str = "list creator's private drafts";
const RESOURCE: &str = "private-drafts-updated-desc";

/// [`sql::draft::PAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Stored {
    id: String,
    title: String,
    status: String,
    resource_version: i64,
    fleet_id: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    expires_at: Option<jiff_sqlx::Timestamp>,
}

impl Drafts {
    /// Lists only this person's private drafts after current membership checks.
    ///
    /// # Errors
    /// Refuses invalid or foreign continuations and inaccessible workspaces.
    pub async fn list(&self, access: Access<'_>, request: ListQuery) -> Result<Page<DraftCard>> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let binding = replay::digest(&(
            RESOURCE,
            access.workspace,
            &person.id,
            request.limit,
            request.state,
            &request.linked_fleet_id,
        ))?;
        let after = paging::boundary(request.starting_after.as_deref(), &binding)?;
        let rows: Vec<Stored> = sqlx::query_as(sql::draft::PAGE)
            .bind(access.workspace.as_str())
            .bind(person.id.as_str())
            .bind(request.state.map(DraftState::as_str))
            .bind(request.linked_fleet_id.as_ref().map(Uuid7::as_str))
            .bind(after.as_ref().map(|(at, _)| at.to_sqlx()))
            .bind(after.as_ref().map(|(_, id)| id.as_str()))
            .bind(i64::from(request.limit) + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let page = project(rows, request.limit as usize, &binding)?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(page)
    }
}

fn project(rows: Vec<Stored>, limit: usize, binding: &str) -> Result<Page<DraftCard>> {
    let more = rows.len() > limit;
    let mut items = Vec::new();
    let mut next_cursor = None;
    for row in rows.into_iter().take(limit) {
        let id = Uuid7::parse(&row.id)?;
        next_cursor = Some(paging::cursor(binding, row.updated_at, &id));
        items.push(DraftCard {
            etag: super::etag(&id, row.resource_version),
            id,
            title: row.title,
            state: super::word(&row.status)?,
            linked_fleet_id: super::read::optional(row.fleet_id.as_deref())?,
            updated_at: row.updated_at,
            expires_at: row.expires_at.map(jiff_sqlx::Timestamp::to_jiff),
        });
    }
    Ok(Page {
        items,
        total: None,
        next_cursor: if more { next_cursor } else { None },
    })
}
