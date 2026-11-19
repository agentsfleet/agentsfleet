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
    types::{Candidate, SaveOrigin, SaveSource, SourcePin},
};
use af_core::id::Uuid7;
use af_fleet_runtime::Definition;
use sqlx::PgConnection;

pub(super) struct Captured {
    pub(super) origin: Option<SaveOrigin>,
    pub(super) candidate: Candidate,
    pub(super) context: review::Context,
    pub(super) draft: Option<draft::read::Locked>,
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
                let id: Option<Uuid7> = sqlx::query_scalar(sql::CANDIDATE_DRAFT)
                    .bind(draft_candidate_id)
                    .bind(access.workspace)
                    .bind(&person.id)
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(query(CONTEXT))?;
                let id = id.ok_or(ErrorKind::Hidden)?;
                let locked = draft::read::lock(connection, access, person, &id).await?;
                if locked.head_candidate_id.as_ref() != Some(draft_candidate_id) {
                    return Err(ErrorKind::State {
                        current: "candidate_changed",
                    }
                    .into());
                }
                let candidate = candidate::read(connection, &id, draft_candidate_id).await?;
                Ok(Captured {
                    origin: None,
                    candidate,
                    context: review::Context::from(&locked),
                    draft: Some(locked),
                })
            }
            SaveSource::Plugin {
                plugin_id,
                revision_id,
            } => plugin(connection, access, person, plugin_id, revision_id).await,
        }
    }
}

/// [`sql::PLUGIN`]'s row, by column name.
#[expect(
    clippy::struct_field_names,
    reason = "`revision` is the `setup.plugin_revisions` column it decodes"
)]
#[derive(sqlx::FromRow)]
struct Revision {
    id: Uuid7,
    revision: i64,
    definition: String,
    canonical_digest: String,
    policy_semantics: String,
}

async fn plugin(
    connection: &mut PgConnection,
    access: Access<'_>,
    person: &Person,
    plugin: &Uuid7,
    revision: &Uuid7,
) -> Result<Captured> {
    let row: Option<Revision> = sqlx::query_as(sql::PLUGIN)
        .bind(plugin)
        .bind(revision)
        .bind(&person.owner)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let row = row.ok_or(ErrorKind::Hidden)?;
    let definition: Definition = serde_json::from_str(&row.definition).map_err(stored)?;
    Ok(Captured {
        origin: None,
        context: review::Context {
            subject: review::Subject::Plugin {
                plugin_id: plugin.to_owned(),
                revision_id: revision.clone(),
            },
            workspace_id: access.workspace.clone(),
            tenant_id: person.owner.clone(),
            creator_id: Some(person.id.clone()),
            instance_name: None,
            linked_fleet_id: None,
            source_pin: Some(SourcePin::Plugin {
                plugin_id: plugin.to_owned(),
                revision_id: revision.clone(),
            }),
        },
        candidate: Candidate {
            id: row.id,
            revision: row.revision.to_string(),
            definition,
            canonical_digest: row.canonical_digest,
            policy_semantics: row.policy_semantics,
        },
        draft: None,
    })
}
