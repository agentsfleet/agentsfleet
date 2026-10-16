//! Creator-only keyset listing binds every filter and the page size.

use crate::{
    Access, Drafts, Result, access,
    error::{ErrorKind, raise::query},
    replay,
    types::{DraftCard, DraftState, ListQuery, Page},
};
use afd_core::{id::Uuid7, paging::Cursor};
use serde::Deserialize;
use sqlx::Connection as _;

const CONTEXT: &str = "list creator's private drafts";
const RESOURCE: &str = "private-drafts-updated-desc";
const PAGE: &str = const_format::concatcp!(
    "\
SELECT to_jsonb(d)::text FROM core.authoring_drafts d
WHERE workspace_id=$1::uuid AND creator_id=$2::uuid
AND (expires_at IS NULL OR expires_at>",
    afd_db::statement::NOW_MS,
    ")
AND ($3::text IS NULL OR state=$3) AND ($4::uuid IS NULL OR linked_fleet_id=$4::uuid)
AND ($5::bigint IS NULL OR (updated_at,id)<($5,$6::uuid))
ORDER BY updated_at DESC,id DESC LIMIT $7",
);

#[derive(Deserialize)]
struct Stored {
    id: Uuid7,
    title: String,
    state: DraftState,
    resource_version: i64,
    linked_fleet_id: Option<Uuid7>,
    updated_at: i64,
    expires_at: Option<i64>,
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
        let after = boundary(request.starting_after.as_deref(), &binding)?;
        let rows: Vec<String> = sqlx::query_scalar(PAGE)
            .bind(access.workspace.as_str())
            .bind(person.id.as_str())
            .bind(request.state.map(DraftState::as_str))
            .bind(request.linked_fleet_id.as_ref().map(Uuid7::as_str))
            .bind(after.as_ref().map(|(at, _)| *at))
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

fn project(rows: Vec<String>, limit: usize, binding: &str) -> Result<Page<DraftCard>> {
    let more = rows.len() > limit;
    let mut items = Vec::new();
    let mut next_cursor = None;
    for row in rows.into_iter().take(limit) {
        let row: Stored = super::value(&row)?;
        next_cursor = Some(
            Cursor::Text {
                value: binding.to_owned(),
                id: format!("{}:{}", row.updated_at, row.id),
            }
            .to_string(),
        );
        items.push(DraftCard {
            etag: super::etag(&row.id, row.resource_version),
            id: row.id,
            title: row.title,
            state: row.state,
            linked_fleet_id: row.linked_fleet_id,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
        });
    }
    Ok(Page {
        items,
        total: None,
        next_cursor: if more { next_cursor } else { None },
    })
}

fn boundary(raw: Option<&str>, binding: &str) -> Result<Option<(i64, Uuid7)>> {
    let Some(raw) = raw else { return Ok(None) };
    let Cursor::Text { value, id } = Cursor::parse(raw).map_err(|_invalid| ErrorKind::Cursor)?
    else {
        return Err(ErrorKind::Cursor.into());
    };
    if value != binding {
        return Err(ErrorKind::Cursor.into());
    }
    let (at, id) = id.split_once(':').ok_or(ErrorKind::Cursor)?;
    let at = at.parse().map_err(|_invalid| ErrorKind::Cursor)?;
    let id = Uuid7::parse(id).map_err(|_invalid| ErrorKind::Cursor)?;
    Ok(Some((at, id)))
}
