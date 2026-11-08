//! A draft is looked up by its creator as well as its workspace.

use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    candidate,
    error::{ErrorKind, raise::query},
    model, sql,
    types::{Draft, DraftState, PinColumns, Publication, SourcePin},
};
use af_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "read private draft";

#[derive(Clone)]
pub(crate) struct Locked {
    pub(crate) id: Uuid7,
    pub(crate) status: DraftState,
    pub(crate) title: String,
    pub(crate) resource_version: i64,
    pub(crate) authoring_model_ref: String,
    pub(crate) head_candidate_id: Option<Uuid7>,
    /// The head candidate's revision; zero before the first candidate.
    pub(crate) candidate_revision: i64,
    pub(crate) last_message_sequence: i64,
    pub(crate) cancelled_through_sequence: i64,
    pub(crate) fleet_id: Option<Uuid7>,
    pub(crate) closed_at: Option<Timestamp>,
    pub(crate) expires_at: Option<Timestamp>,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    pub(crate) user_id: Uuid7,
    pub(crate) instance_name: Option<String>,
    pub(crate) source_pin: Option<SourcePin>,
    pub(crate) publication: Option<Publication>,
}

/// [`sql::draft::LOCK`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct LockedRow {
    id: Uuid7,
    status: String,
    title: String,
    resource_version: i64,
    tenant_model_id: Option<Uuid7>,
    head_candidate_id: Option<Uuid7>,
    candidate_revision: i64,
    last_message_sequence: i64,
    cancelled_through_sequence: i64,
    fleet_id: Option<Uuid7>,
    closed_at: Option<jiff_sqlx::Timestamp>,
    expires_at: Option<jiff_sqlx::Timestamp>,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    user_id: Uuid7,
    instance_name: Option<String>,
    source_type: String,
    source_library_id: Option<String>,
    source_library_revision_id: Option<Uuid7>,
    source_fleet_id: Option<Uuid7>,
    source_fleet_revision: Option<i64>,
    publication_library_id: Option<String>,
    publication_revision_id: Option<Uuid7>,
    publication_digest: Option<String>,
}

impl LockedRow {
    fn decode(self) -> Result<Locked> {
        let library = pair(
            self.source_library_id,
            self.source_library_revision_id.map(String::from),
        )?;
        let fleet = match (self.source_fleet_id, self.source_fleet_revision) {
            (Some(fleet), Some(revision)) => Some((fleet, revision)),
            _ => None,
        };
        let source_pin = SourcePin::from_columns(PinColumns {
            source_type: self.source_type,
            library,
            fleet,
        })?;
        let publication = match (
            pair(
                self.publication_library_id,
                self.publication_revision_id.map(String::from),
            )?,
            self.publication_digest,
        ) {
            (Some((library_id, revision_id)), Some(canonical_digest)) => Some(Publication {
                library_id,
                revision_id,
                canonical_digest,
            }),
            _ => None,
        };
        Ok(Locked {
            id: self.id.clone(),
            status: crate::draft::word(&self.status)?,
            title: self.title,
            resource_version: self.resource_version,
            authoring_model_ref: model::reference(self.tenant_model_id),
            head_candidate_id: optional(self.head_candidate_id.as_ref().map(Uuid7::as_str))?,
            candidate_revision: self.candidate_revision,
            last_message_sequence: self.last_message_sequence,
            cancelled_through_sequence: self.cancelled_through_sequence,
            fleet_id: optional(self.fleet_id.as_ref().map(Uuid7::as_str))?,
            closed_at: self.closed_at.map(jiff_sqlx::Timestamp::to_jiff),
            expires_at: self.expires_at.map(jiff_sqlx::Timestamp::to_jiff),
            workspace_id: self.workspace_id.clone(),
            tenant_id: self.tenant_id.clone(),
            user_id: self.user_id.clone(),
            instance_name: self.instance_name,
            source_pin,
            publication,
        })
    }
}

/// A public Library id beside the revision it names, when the join found both.
fn pair(library: Option<String>, revision: Option<String>) -> Result<Option<(String, Uuid7)>> {
    Ok(match (library, revision) {
        (Some(library), Some(revision)) => Some((library, Uuid7::parse(&revision)?)),
        _ => None,
    })
}

pub(crate) fn optional(id: Option<&str>) -> Result<Option<Uuid7>> {
    Ok(id.map(Uuid7::parse).transpose()?)
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
    let row: Option<LockedRow> = sqlx::query_as(sql::draft::LOCK)
        .bind(id)
        .bind(access.workspace)
        .bind(&person.id)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::Hidden)?.decode()
}

pub(crate) async fn project(connection: &mut PgConnection, locked: Locked) -> Result<Draft> {
    let candidate = match locked.head_candidate_id {
        Some(ref id) => Some(candidate::read(connection, &locked.id, id).await?),
        None => None,
    };
    Ok(Draft {
        etag: super::etag(&locked.id, locked.resource_version),
        id: locked.id,
        state: locked.status,
        title: locked.title,
        authoring_model_ref: locked.authoring_model_ref,
        cancelled_through: locked.cancelled_through_sequence.to_string(),
        candidate,
        review: None,
        linked_fleet_id: locked.fleet_id,
        publication: locked.publication,
        closed_at: locked.closed_at,
        expires_at: locked.expires_at,
    })
}
