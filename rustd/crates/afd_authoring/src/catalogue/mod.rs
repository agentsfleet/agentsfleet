//! Owner model choices use one connection, one bounded statement and no key reads.

mod project;
mod sql;

use crate::{
    Access, Drafts, Result, access,
    error::{ErrorKind, raise::query},
    replay,
    types::{ModelsQuery, Page, WorkspaceModel},
};
use afd_core::paging::Cursor;
use afd_credential::provider::PLATFORM_MODEL_REF;
use sqlx::Connection as _;

const CONTEXT: &str = "list workspace owner's model choices";
const RESOURCE: &str = "workspace-owner-models-reference-asc";

impl Drafts {
    /// Lists configured priced choices from the workspace owner, including for joined members.
    ///
    /// # Errors
    /// Preserves store/validation failures and refuses foreign continuation bindings.
    pub async fn models(
        &self,
        access: Access<'_>,
        request: ModelsQuery,
    ) -> Result<Page<WorkspaceModel>> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let binding = replay::digest(&(
            RESOURCE,
            access.workspace,
            &person.owner,
            &person.id,
            request.purpose,
            request.limit,
        ))?;
        let after = boundary(request.starting_after.as_deref(), &binding)?;
        let rows: Vec<project::Row> = sqlx::query_as(sql::PAGE)
            .bind(person.owner.as_str())
            .bind(PLATFORM_MODEL_REF)
            .bind(after)
            .bind(i64::from(request.limit) + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let page = page(rows, &request, &binding);
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(page)
    }
}

fn boundary(raw: Option<&str>, binding: &str) -> Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    match Cursor::parse(raw).map_err(|_invalid| ErrorKind::Cursor)? {
        Cursor::Text { value, id } if value == binding => Ok(Some(id)),
        _ => Err(ErrorKind::Cursor.into()),
    }
}

fn page(rows: Vec<project::Row>, request: &ModelsQuery, binding: &str) -> Page<WorkspaceModel> {
    let more = rows.len() > request.limit as usize;
    let mut items = Vec::new();
    let mut last = None;
    for row in rows.into_iter().take(request.limit as usize) {
        last = Some(
            Cursor::Text {
                value: binding.to_owned(),
                id: row.reference.clone(),
            }
            .to_string(),
        );
        if let Some(item) = project::eligible(row, request.purpose) {
            items.push(item);
        }
    }
    Page {
        items,
        total: None,
        next_cursor: if more { last } else { None },
    }
}
