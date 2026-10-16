//! A draft is looked up by its creator as well as its workspace.

use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    candidate,
    error::{ErrorKind, raise::query},
    sql,
    types::{Draft, DraftState},
};
use afd_core::id::Uuid7;
use serde::Deserialize;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "read private draft";

#[derive(Deserialize)]
pub(crate) struct Locked {
    pub(crate) id: Uuid7,
    pub(crate) state: DraftState,
    pub(crate) title: String,
    pub(crate) resource_version: i64,
    pub(crate) authoring_model_ref: String,
    pub(crate) current_candidate_id: Option<Uuid7>,
    pub(crate) candidate_revision: i64,
    pub(crate) last_sequence: i64,
    pub(crate) cancelled_through: i64,
    pub(crate) deletion_generation: i64,
    pub(crate) linked_fleet_id: Option<Uuid7>,
    pub(crate) closed_at: Option<i64>,
    pub(crate) expires_at: Option<i64>,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    pub(crate) creator_id: Uuid7,
    pub(crate) instance_name: Option<String>,
    pub(crate) source_pin: Option<crate::types::SourcePin>,
    pub(crate) publication: Option<crate::types::Publication>,
}

impl Drafts {
    /// Reads only the authenticated creator's current private setup.
    ///
    /// # Errors
    /// Hides missing/foreign/revoked resources and propagates datastore faults.
    pub async fn read(&self, access: Access<'_>, id: &Uuid7) -> Result<Draft> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = lock(&mut tx, access, &person, id).await?;
        let draft = project(&mut tx, locked).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(draft)
    }
}

pub(crate) async fn lock(
    connection: &mut PgConnection,
    access: Access<'_>,
    person: &Person,
    id: &Uuid7,
) -> Result<Locked> {
    let value: Option<String> = sqlx::query_scalar(sql::draft::LOCK)
        .bind(id.as_str())
        .bind(access.workspace.as_str())
        .bind(person.id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    super::value(&value.ok_or(ErrorKind::Hidden)?)
}

pub(crate) async fn project(connection: &mut PgConnection, locked: Locked) -> Result<Draft> {
    let candidate = match locked.current_candidate_id {
        Some(ref id) => Some(candidate::read(connection, &locked.id, id).await?),
        None => None,
    };
    Ok(Draft {
        etag: super::etag(&locked.id, locked.resource_version),
        id: locked.id,
        state: locked.state,
        title: locked.title,
        authoring_model_ref: locked.authoring_model_ref,
        cancelled_through: locked.cancelled_through.to_string(),
        candidate,
        review: None,
        linked_fleet_id: locked.linked_fleet_id,
        publication: locked.publication,
        closed_at: locked.closed_at,
        expires_at: locked.expires_at,
    })
}
