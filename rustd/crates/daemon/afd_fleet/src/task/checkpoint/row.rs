//! Private SQL projections are checked before producing protocol values.

use crate::{
    Result,
    error::query,
    task::{TaskRefusal, execution, row::counter},
};
use af_core::id::Uuid7;
use af_wire::task::{CheckpointMode, CheckpointPin, CheckpointPlan, CheckpointTransferPurpose};
use sqlx::PgConnection;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct Plan {
    pub id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub fleet_id: String,
    pub generation: i64,
    pub task_generation: i64,
    pub stop_generation: i64,
    pub request_digest: String,
    pub mode: String,
    pub repository: String,
    #[sqlx(rename = "task_base_id")]
    pub base_id: Option<String>,
    pub format_version: i32,
    pub toolbox_digest: String,
    pub page_count: i32,
    pub page_ids: Vec<String>,
    pub upload_budget_bytes: i64,
    pub uploaded_bytes: i64,
    pub base_page_ids: Vec<String>,
    pub base_manifest_root: Option<String>,
    pub base_commit_oid: Option<String>,
    pub logical_bytes: i64,
    pub entry_count: i32,
    pub manifest_root: Option<String>,
    pub history_digest: Option<String>,
    pub covered_input_sequence: i64,
    pub effect_watermark: i64,
    pub owner_lease_id: String,
    pub owner_fence: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    #[sqlx(rename = "transfer_expires_at")]
    pub transfer_deadline: jiff::Timestamp,
    pub ready: bool,
    pub abandoned: bool,
}

impl Plan {
    pub(super) async fn load(
        connection: &mut PgConnection,
        id: &Uuid7,
        task: &str,
    ) -> Result<Self> {
        sqlx::query_as(include_str!("sql/plan.sql"))
            .bind(id.as_str())
            .bind(task)
            .fetch_optional(connection)
            .await
            .map_err(query("lock task checkpoint plan"))?
            .ok_or_else(|| TaskRefusal::NotFound.into())
    }

    pub(super) fn mode(&self) -> Result<CheckpointMode> {
        match self.mode.as_str() {
            "retain_base" => Ok(CheckpointMode::RetainBase),
            "incremental" => Ok(CheckpointMode::Incremental),
            _ => Err(TaskRefusal::CheckpointIncompatible.into()),
        }
    }

    // A reader may inspect a ready retained base through its task-scoped plan.
    // Only incremental checkpoints are installed as a task recovery boundary.
    pub(super) fn compatible(
        &self,
        owned: &execution::Owned,
        purpose: CheckpointTransferPurpose,
    ) -> Result<()> {
        let version = counter(self.task_generation, "checkpoint task generation")?;
        if self.abandoned
            || self.format_version
                != i32::try_from(af_wire::task::CHECKPOINT_FORMAT)
                    .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?
            || version > owned.snapshot.context.generation
            || purpose == CheckpointTransferPurpose::Upload
                && (self.ready
                    || version != owned.snapshot.context.generation
                    || self.stop_generation
                        != i64::try_from(owned.snapshot.context.stop_generation)
                            .map_err(|_overflow| TaskRefusal::StaleState)?)
            || purpose == CheckpointTransferPurpose::Restore && !self.ready
        {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        Ok(())
    }

    pub(super) fn wire(&self, pin: &Pin, replayed: bool) -> Result<CheckpointPlan> {
        if usize::try_from(self.page_count).ok() != Some(self.page_ids.len()) {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        Ok(CheckpointPlan {
            checkpoint_id: self.id.clone(),
            generation: counter(self.generation, "checkpoint generation")?,
            base_id: self.base_id.clone(),
            mode: self.mode()?,
            repository: self.repository.clone(),
            format_version: u32::try_from(self.format_version)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
            toolbox_digest: self.toolbox_digest.clone(),
            page_ids: self.page_ids.clone(),
            base_page_ids: self.base_page_ids.clone(),
            base_manifest_root: self.base_manifest_root.clone(),
            base_commit_oid: self.base_commit_oid.clone(),
            manifest_root: self.manifest_root.clone(),
            history_digest: self.history_digest.clone(),
            covered_input_sequence: counter(
                self.covered_input_sequence,
                "checkpoint input sequence",
            )?,
            effect_watermark: counter(self.effect_watermark, "checkpoint effect watermark")?,
            expires_at: self.expires_at,
            pin: pin.wire()?,
            replayed,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct Pin {
    pub id: String,
    pub kind: String,
    pub owner_lease_id: String,
    pub owner_fence: i64,
    pub task_generation: i64,
    pub stop_generation: i64,
    pub resource_version: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    #[sqlx(rename = "operation_expires_at")]
    pub operation_deadline: jiff::Timestamp,
    pub released: bool,
}

impl Pin {
    pub(super) fn wire(&self) -> Result<CheckpointPin> {
        Ok(CheckpointPin {
            id: self.id.clone(),
            resource_version: counter(self.resource_version, "pin version")?,
            operation_deadline: self.operation_deadline,
            expires_at: self.expires_at,
        })
    }
}
