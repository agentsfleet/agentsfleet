//! Pending plan rows are scoped to the proved event before they become responses.

use af_core::id::Uuid7;
use af_wire::run::{FoldRef, TurnCheckpointPlan, TurnId, TurnResume, TurnScope, TurnWorkspace};
use garde::Validate as _;

use super::{TurnCheckpointRefusal, guard::TurnRunGuard, pins::Pin};
use crate::{Result, error::query};

#[derive(sqlx::FromRow)]
pub(super) struct Plan {
    pub id: Uuid7,
    pub setup_revision_id: Uuid7,
    pub owner_run_id: Uuid7,
    pub owner_fence: i64,
    #[sqlx(try_from = "i64")]
    pub generation: u64,
    #[sqlx(try_from = "i32")]
    pub format_version: u32,
    pub expected_head_id: Option<Uuid7>,
    pub has_workspace: bool,
    pub toolbox_digest: Option<String>,
    pub page_ids: Vec<Uuid7>,
    pub history_digest: Option<String>,
    pub manifest_root: Option<String>,
    pub covered_fold_ids: Option<Vec<Uuid7>>,
    pub covered_fold_sequences: Option<Vec<i64>>,
    pub ready: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
    pub abandoned: bool,
    pub upload_budget_bytes: i64,
    pub uploaded_bytes: i64,
    pub logical_bytes: i64,
    pub entry_count: i32,
}

impl Plan {
    pub(super) async fn load(owner: &mut TurnRunGuard<'_>, id: &Uuid7) -> Result<Self> {
        sqlx::query_as(include_str!("sql/plan.sql"))
            .bind(id)
            .bind(&owner.current.event_id)
            .bind(&owner.current.fleet_id)
            .bind(&owner.current.workspace_id)
            .fetch_optional(&mut *owner.tx)
            .await
            .map_err(query("hold event-owned turn checkpoint"))?
            .ok_or_else(|| TurnCheckpointRefusal::Conflict.into())
    }

    pub(super) fn wire(
        &self,
        owner: &TurnRunGuard<'_>,
        pin: &Pin,
        replayed: bool,
    ) -> Result<TurnCheckpointPlan> {
        if self.setup_revision_id != owner.setup
            || self.abandoned
            || self.owner_run_id != owner.current.id
            || self.owner_fence != owner.current.fence
        {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        let workspace = match (self.has_workspace, &self.toolbox_digest) {
            (false, None) => TurnWorkspace::ApiOnly,
            (true, Some(digest)) => TurnWorkspace::Files {
                toolbox_digest: digest.clone(),
            },
            _ => return Err(TurnCheckpointRefusal::Incompatible.into()),
        };
        Ok(TurnCheckpointPlan {
            checkpoint_id: TurnId(self.id.as_str().to_owned()),
            generation: self.generation,
            format_version: self.format_version,
            scope: owner.scope(workspace),
            expected_head: self
                .expected_head_id
                .as_ref()
                .map(|id| TurnId(id.as_str().to_owned())),
            page_ids: self
                .page_ids
                .iter()
                .map(|id| TurnId(id.as_str().to_owned()))
                .collect(),
            pin: pin.wire(),
            history_digest: self
                .history_digest
                .clone()
                .ok_or(TurnCheckpointRefusal::Incompatible)?,
            replayed,
        })
    }

    pub(super) fn scope(&self, owner: &TurnRunGuard<'_>) -> Result<TurnScope> {
        if self.setup_revision_id != owner.setup {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
        let workspace = match (self.has_workspace, &self.toolbox_digest) {
            (false, None) => TurnWorkspace::ApiOnly,
            (true, Some(toolbox_digest)) => TurnWorkspace::Files {
                toolbox_digest: toolbox_digest.clone(),
            },
            _ => return Err(TurnCheckpointRefusal::Incompatible.into()),
        };
        Ok(owner.scope(workspace))
    }

    pub(super) fn folds(&self) -> Result<Vec<FoldRef>> {
        let ids = self
            .covered_fold_ids
            .as_ref()
            .ok_or(TurnCheckpointRefusal::Incompatible)?;
        let sequences = self
            .covered_fold_sequences
            .as_ref()
            .ok_or(TurnCheckpointRefusal::Incompatible)?;
        if ids.len() != sequences.len() {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
        Ok(ids
            .iter()
            .zip(sequences)
            .map(|(id, sequence)| FoldRef {
                event_id: TurnId(id.as_str().to_owned()),
                sequence: *sequence,
            })
            .collect())
    }

    pub(super) fn descriptor(&self, owner: &TurnRunGuard<'_>) -> Result<TurnResume> {
        if !self.ready || self.abandoned {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
        let descriptor = TurnResume {
            checkpoint_id: TurnId(self.id.as_str().to_owned()),
            generation: self.generation,
            format_version: self.format_version,
            scope: self.scope(owner)?,
            manifest_root: self
                .manifest_root
                .clone()
                .ok_or(TurnCheckpointRefusal::Incompatible)?,
            history_digest: self
                .history_digest
                .clone()
                .ok_or(TurnCheckpointRefusal::Incompatible)?,
        };
        descriptor
            .validate()
            .map_err(super::root::invalid("stored turn descriptor"))?;
        Ok(descriptor)
    }
}
