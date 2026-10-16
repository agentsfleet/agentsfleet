//! Sources are authorized and validated before reserving a fleet identity.

use super::{CONTEXT, Creations, sql};
use crate::{
    Access, Result,
    access::Person,
    candidate, draft,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    review,
    types::{Candidate, SaveSource, SourcePin},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use serde::Deserialize;
use sqlx::PgConnection;

pub(super) struct Captured {
    pub(super) candidate: Candidate,
    pub(super) context: review::Context,
    pub(super) draft: Option<draft::read::Locked>,
    pub(super) library_revision: Option<Uuid7>,
}

impl Creations {
    pub(super) async fn capture(
        &self,
        connection: &mut PgConnection,
        access: Access<'_>,
        person: &Person,
        source: &SaveSource,
    ) -> Result<Captured> {
        match source {
            SaveSource::Draft { draft_candidate_id } => {
                let id: Option<String> = sqlx::query_scalar(sql::CANDIDATE_DRAFT)
                    .bind(draft_candidate_id.as_str())
                    .bind(access.workspace.as_str())
                    .bind(person.id.as_str())
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(query(CONTEXT))?;
                let id = Uuid7::parse(&id.ok_or(ErrorKind::Hidden)?)?;
                let locked = draft::read::lock(connection, access, person, &id).await?;
                if locked.current_candidate_id.as_ref() != Some(draft_candidate_id) {
                    return Err(ErrorKind::State {
                        current: "candidate_changed",
                    }
                    .into());
                }
                let candidate = candidate::read(connection, &id, draft_candidate_id).await?;
                let published = published(connection, &locked, person, &candidate).await?;
                Ok(Captured {
                    candidate,
                    context: review::Context::from(&locked),
                    draft: Some(locked),
                    library_revision: published,
                })
            }
            SaveSource::Library {
                library_id,
                revision_id,
            } => library(connection, access, person, library_id, revision_id).await,
        }
    }
}

async fn published(
    connection: &mut PgConnection,
    draft: &draft::read::Locked,
    person: &Person,
    candidate: &Candidate,
) -> Result<Option<Uuid7>> {
    let Some(receipt) = draft
        .publication
        .as_ref()
        .filter(|receipt| receipt.canonical_digest == candidate.canonical_digest)
    else {
        return Ok(None);
    };
    let id: Option<String> = sqlx::query_scalar(sql::PUBLISHED)
        .bind(receipt.library_id.as_str())
        .bind(receipt.revision_id.as_str())
        .bind(&candidate.canonical_digest)
        .bind(&candidate.policy_semantics)
        .bind(person.owner.as_str())
        .bind(crate::library::PLATFORM)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    id.map(|id| Uuid7::parse(&id).map_err(Into::into))
        .transpose()
}

#[expect(
    clippy::struct_field_names,
    reason = "`revision` is the `core.fleet_library_revisions` column, decoded from `to_jsonb(r)`"
)]
#[derive(Deserialize)]
struct Revision {
    id: Uuid7,
    revision: i64,
    definition: Definition,
    canonical_digest: String,
    policy_semantics: String,
}

async fn library(
    connection: &mut PgConnection,
    access: Access<'_>,
    person: &Person,
    library: &str,
    revision: &Uuid7,
) -> Result<Captured> {
    let row: Option<String> = sqlx::query_scalar(sql::LIBRARY)
        .bind(library)
        .bind(revision.as_str())
        .bind(person.owner.as_str())
        .bind(crate::library::PLATFORM)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let row: Revision = serde_json::from_str(&row.ok_or(ErrorKind::Hidden)?).map_err(stored)?;
    Ok(Captured {
        context: review::Context {
            subject: review::Subject::Library {
                library_id: library.to_owned(),
                revision_id: revision.clone(),
            },
            workspace_id: access.workspace.clone(),
            tenant_id: person.owner.clone(),
            creator_id: Some(person.id.clone()),
            instance_name: None,
            linked_fleet_id: None,
            source_pin: Some(SourcePin::Library {
                library_id: library.to_owned(),
                revision_id: revision.clone(),
            }),
        },
        candidate: Candidate {
            id: row.id,
            revision: row.revision.to_string(),
            definition: row.definition,
            canonical_digest: row.canonical_digest,
            policy_semantics: row.policy_semantics,
        },
        draft: None,
        library_revision: Some(revision.clone()),
    })
}
