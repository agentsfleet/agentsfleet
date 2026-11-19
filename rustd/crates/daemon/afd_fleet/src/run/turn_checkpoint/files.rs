//! A concrete file operation owns ordinary authority before quota or object locks.

use af_wire::{run::TurnCheckpointRef, task::CheckpointTransferPurpose};
use sqlx::PgConnection;

use super::{
    TurnCheckpointRefusal, TurnRun, allocation,
    guard::{self, TurnRunGuard},
    pins::{self, Pin},
    row::Plan,
};
use crate::{Result, run::Runs};

pub(super) struct FileOwner<'a> {
    pub owner: TurnRunGuard<'a>,
    pub plan: Plan,
    pub pin: Pin,
    pub namespace: String,
    purpose: CheckpointTransferPurpose,
}

impl<'a> FileOwner<'a> {
    pub(super) async fn lock(
        runs: &Runs,
        connection: &'a mut PgConnection,
        presented: TurnRun<'_>,
        reference: &TurnCheckpointRef,
        purpose: CheckpointTransferPurpose,
    ) -> Result<Self> {
        let mut owner = TurnRunGuard::lock(runs, connection, presented).await?;
        let head = allocation::head(&mut owner).await?;
        let plan = Plan::load(&mut owner, &guard::id(&reference.checkpoint_id)?).await?;
        plan.scope(&owner)?;
        let pin = pins::exact(&mut owner, reference).await?;
        let kind = match purpose {
            CheckpointTransferPurpose::Upload => super::WRITER,
            CheckpointTransferPurpose::Restore => super::READER,
        };
        if !plan.has_workspace || !pin.belongs(&owner, kind) || plan.abandoned {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        match purpose {
            CheckpointTransferPurpose::Upload
                if plan.ready
                    || plan.owner_run_id != owner.current.id
                    || plan.owner_fence != owner.current.fence =>
            {
                return Err(TurnCheckpointRefusal::Conflict.into());
            }
            CheckpointTransferPurpose::Restore
                if !plan.ready || head.as_ref() != Some(&plan.id) =>
            {
                return Err(TurnCheckpointRefusal::Conflict.into());
            }
            _ => {}
        }
        let namespace = format!("turn:{}", owner.current.event_id);
        let mut locked = Self {
            owner,
            plan,
            pin,
            namespace,
            purpose,
        };
        locked.fresh().await?;
        Ok(locked)
    }

    pub(super) async fn fresh(&mut self) -> Result<()> {
        let now = self.owner.fresh().await?;
        if !self.pin.live(now)
            || self.purpose == CheckpointTransferPurpose::Upload && self.plan.expires_at <= now
        {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        Ok(())
    }

    pub(super) async fn commit(mut self) -> Result<()> {
        self.fresh().await?;
        self.owner.commit().await
    }
}

pub(super) fn public_failure(fault: crate::Error) -> crate::Error {
    use af_core::error_code;
    let authenticated_state = matches!(fault.kind(), crate::error::ErrorKind::Entropy { source }
        if source.is_open_failed() || source.is_malformed_envelope() || source.is_mac_mismatch());
    let reason = match fault.code() {
        _ if authenticated_state => TurnCheckpointRefusal::Incompatible,
        error_code::TASK_LIMIT_EXHAUSTED => TurnCheckpointRefusal::Limit,
        error_code::TASK_STORAGE_UNAVAILABLE => TurnCheckpointRefusal::Unavailable,
        error_code::TASK_CHECKPOINT_INCOMPATIBLE => TurnCheckpointRefusal::Incompatible,
        error_code::TASK_REPLAY_CONFLICT | error_code::TASK_STALE_STATE => {
            TurnCheckpointRefusal::Conflict
        }
        _ => return fault,
    };
    crate::error::ErrorKind::TurnCheckpointFailure {
        reason,
        context: "private file custody",
        source: Box::new(fault),
    }
    .into()
}
