//! Idle edits revalidate the full candidate and advance its immutable head once.

use super::{etag, read};
use crate::{
    Access, Drafts, Result, access, candidate,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    model, sql,
    types::{Draft, DraftPatch, DraftState, QUEUED, RUNNING},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_fleet_runtime::definition::{ConfigOp, EditBatch};
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "edit private draft";
const EMPTY_PATCH: &str = "a draft change must contain at least one supported field";
const NO_CANDIDATE: &str = "initialize a candidate before editing its setup";
const REVISION_EXHAUSTED: &str = "candidate revision is exhausted";
const AUTHORING: &str = "authoring";
const RUNTIME_MODEL_PATH: &str = "/config/x-agentsfleet/model";

impl Drafts {
    /// Applies one complete drawer mutation under the current strong tag.
    ///
    /// # Errors
    /// Refuses stale versions, active messages, unavailable selections and any
    /// invalid edit; the original candidate and selection survive each refusal.
    pub async fn patch(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        expected: Option<&str>,
        patch: DraftPatch,
        now: UnixMillis,
    ) -> Result<Draft> {
        let patch = garde::Unvalidated::new(patch).validate()?.into_inner();
        nonempty(&patch)?;
        if let Some(name) = patch.instance_name.as_deref() {
            afd_fleet_runtime::FleetName::parse(name)?;
        }
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = read::lock(&mut tx, access, &person, id).await?;
        let current = etag(id, locked.resource_version);
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        if expected != current {
            return Err(ErrorKind::Stale { etag: current }.into());
        }
        idle(&mut tx, &locked).await?;
        let authoring = patch
            .authoring_model_ref
            .as_deref()
            .unwrap_or(&locked.authoring_model_ref);
        model::selected(&mut tx, &person.owner, authoring).await?;
        let title = patch.title.as_deref().unwrap_or(&locked.title);
        sqlx::query(sql::draft::PATCH)
            .bind(id.as_str())
            .bind(title)
            .bind(authoring)
            .bind(&patch.runtime_model_ref)
            .bind(now.as_millis())
            .bind(&patch.instance_name)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        self.patch_candidate(&mut tx, &locked, patch, now).await?;
        let updated = read::lock(&mut tx, access, &person, id).await?;
        let draft = read::project(&mut tx, updated).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(draft)
    }

    async fn patch_candidate(
        &self,
        connection: &mut PgConnection,
        locked: &read::Locked,
        mut patch: DraftPatch,
        now: UnixMillis,
    ) -> Result<()> {
        if let Some(reference) = patch.runtime_model_ref.take() {
            patch.config_ops.push(ConfigOp::Add {
                path: RUNTIME_MODEL_PATH.to_owned(),
                value: reference.into(),
            });
        }
        if patch.config_ops.is_empty() && patch.instruction_edits.is_empty() {
            return Ok(());
        }
        let base = candidate::read(
            connection,
            &locked.id,
            locked
                .current_candidate_id
                .as_ref()
                .ok_or_else(|| invalid(NO_CANDIDATE))?,
        )
        .await?;
        let definition = base.definition.edit(EditBatch {
            config_ops: patch.config_ops,
            instruction_edits: patch.instruction_edits,
        })?;
        let revision = locked
            .candidate_revision
            .checked_add(1)
            .ok_or_else(|| invalid(REVISION_EXHAUSTED))?;
        candidate::append(
            connection,
            candidate::Replacement {
                id: &self.mint(now)?,
                draft: &locked.id,
                revision,
                definition: &definition,
                now,
            },
        )
        .await?;
        Ok(())
    }
}

pub(crate) async fn idle(connection: &mut PgConnection, locked: &read::Locked) -> Result<()> {
    if locked.state != DraftState::Editing {
        return Err(ErrorKind::State {
            current: locked.state.as_str(),
        }
        .into());
    }
    let active: bool = sqlx::query_scalar(sql::draft::ACTIVE_MESSAGES)
        .bind(locked.id.as_str())
        .bind(QUEUED)
        .bind(RUNNING)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    if active {
        return Err(ErrorKind::State { current: AUTHORING }.into());
    }
    Ok(())
}

fn nonempty(patch: &DraftPatch) -> Result<()> {
    if patch.title.is_none()
        && patch.instance_name.is_none()
        && patch.authoring_model_ref.is_none()
        && patch.runtime_model_ref.is_none()
        && patch.config_ops.is_empty()
        && patch.instruction_edits.is_empty()
    {
        return Err(invalid(EMPTY_PATCH));
    }
    Ok(())
}
