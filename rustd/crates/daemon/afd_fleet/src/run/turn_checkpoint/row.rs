//! Pending plan rows are scoped to the proved event before they become responses.

use af_core::id::Uuid7;
use af_wire::run::{
    CheckpointOperationId, FoldRef, TurnCheckpointPlan, TurnResume, TurnScope, TurnWorkspace,
};
use garde::Validate as _;

use super::{TurnCheckpointRefusal, guard::TurnRunGuard, pins::Pin};
use crate::durable_task::checkpoint::sandbox::recorded;
use crate::{Result, error::query};

#[derive(sqlx::FromRow)]
pub(super) struct Plan {
    pub id: Uuid7,
    pub owner_run_id: Uuid7,
    pub owner_fence: i64,
    #[sqlx(try_from = "i64")]
    pub generation: u64,
    #[sqlx(try_from = "i32")]
    pub format_version: u32,
    /// The cipher this plan's new objects and its root are sealed under, as recorded.
    pub cipher: String,
    pub expected_head_id: Option<Uuid7>,
    /// The generation this plan's pages are a delta against; none for a full manifest.
    pub parent_id: Option<Uuid7>,
    pub has_workspace: bool,
    pub toolbox_digest: Option<String>,
    /// The engine the files were taken in; present exactly when a toolbox is
    /// (`ck_checkpoints_turn_sandbox`).
    pub engine: Option<String>,
    pub guest_agent_digest: Option<String>,
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
            .bind(&owner.current.turn_id)
            .bind(&owner.current.fleet_id)
            .bind(&owner.current.workspace_id)
            .fetch_optional(&mut *owner.tx)
            .await
            .map_err(query("hold event-owned turn checkpoint"))?
            .ok_or_else(|| TurnCheckpointRefusal::Conflict.into())
    }

    /// The plan as its writer is answered. Both callers hold its writer pin,
    /// proved this run's own and live, so the plan is this run's and pending.
    pub(super) fn wire(
        &self,
        owner: &TurnRunGuard<'_>,
        pin: &Pin,
        replayed: bool,
    ) -> Result<TurnCheckpointPlan> {
        Ok(TurnCheckpointPlan {
            checkpoint_id: CheckpointOperationId(self.id.as_str().to_owned()),
            generation: self.generation,
            format_version: self.format_version,
            scope: self.scope(owner)?,
            expected_head: self
                .expected_head_id
                .as_ref()
                .map(|id| CheckpointOperationId(id.as_str().to_owned())),
            parent: self
                .parent_id
                .as_ref()
                .map(|id| CheckpointOperationId(id.as_str().to_owned())),
            page_ids: self
                .page_ids
                .iter()
                .map(|id| CheckpointOperationId(id.as_str().to_owned()))
                .collect(),
            pin: pin.wire(),
            history_digest: self
                .history_digest
                .clone()
                .ok_or(TurnCheckpointRefusal::Incompatible)?,
            replayed,
        })
    }

    /// The plan's scope: its workspace is the files it saves and the sandbox
    /// they were taken in, or none.
    ///
    /// # Errors
    /// A toolbox with no recorded engine, or an engine this build does not
    /// name: the files restore nowhere it runs.
    pub(super) fn scope(&self, owner: &TurnRunGuard<'_>) -> Result<TurnScope> {
        // `plan.sql` reads `has_workspace` as a toolbox digest's presence, so
        // the digest names the workspace.
        let workspace = match self.toolbox_digest.clone() {
            None => TurnWorkspace::ApiOnly,
            Some(toolbox_digest) => TurnWorkspace::Files {
                toolbox_digest,
                sandbox: recorded(
                    self.engine.as_deref(),
                    self.guest_agent_digest.as_deref(),
                    TurnCheckpointRefusal::Incompatible,
                )?
                .ok_or(TurnCheckpointRefusal::Incompatible)?,
            },
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
        // `ck_checkpoints_folds` holds both arrays to one length.
        Ok(ids
            .iter()
            .zip(sequences)
            .map(|(id, sequence)| FoldRef {
                turn_id: CheckpointOperationId(id.as_str().to_owned()),
                sequence: *sequence,
            })
            .collect())
    }

    /// A committed plan's descriptor. Only commit writes `manifest_root`, and
    /// `ck_checkpoints_disposition` keeps a committed plan unabandoned, so
    /// a plan never committed refuses at its missing manifest.
    pub(super) fn descriptor(&self, owner: &TurnRunGuard<'_>) -> Result<TurnResume> {
        let descriptor = TurnResume {
            checkpoint_id: CheckpointOperationId(self.id.as_str().to_owned()),
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
