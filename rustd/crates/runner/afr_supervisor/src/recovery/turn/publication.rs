//! Keep the exact uncertain commit until a matching receipt or abandonment settles it.

use super::NativeTurn;
use crate::{Result, error};
use af_wire::lease::{
    TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointRef,
    TurnCheckpointResponse as Response, TurnResume,
};
use zeroize::Zeroize as _;

#[derive(Debug, Default)]
pub(super) struct Publication {
    pub(super) head: Option<TurnResume>,
    pub(super) pending: Option<Pending>,
}

#[derive(Debug)]
pub(super) struct Pending {
    request: TurnCheckpointCommand,
    expected: TurnResume,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Action::Commit { root, .. } = &mut self.request.action {
            root.history.zeroize();
        }
    }
}

impl NativeTurn<'_> {
    pub(super) async fn reconcile(&mut self) -> Result<()> {
        let Some(pending) = &self.publication.pending else {
            return Ok(());
        };
        let Action::Commit { checkpoint, .. } = &pending.request.action else {
            return Err(error::config("uncertain turn publication is not a commit"));
        };
        let reference = checkpoint.clone();
        match self.acknowledge().await {
            Ok(_) => Ok(()),
            Err(failure) if failure.is_retryable() => Err(failure),
            Err(failure) => self.settle_refusal(reference, failure).await,
        }
    }

    async fn settle_refusal(
        &mut self,
        reference: TurnCheckpointRef,
        failure: crate::Error,
    ) -> Result<()> {
        if matches!(
            self.command(Action::Abandon {
                checkpoint: reference
            })
            .await,
            Ok(Response::Abandoned)
        ) {
            self.publication.pending = None;
            return Ok(());
        }
        Err(error::boundary(
            "uncertain turn commit could not be reconciled",
            failure,
        ))
    }

    pub(super) async fn commit_capture(
        &mut self,
        action: Action,
        expected: TurnResume,
    ) -> Result<TurnResume> {
        self.publication.pending = Some(Pending {
            request: self.request(action)?,
            expected,
        });
        let result = self.acknowledge().await;
        if result
            .as_ref()
            .is_err_and(|failure| !failure.is_retryable())
        {
            self.publication.pending = None;
        }
        result
    }

    async fn acknowledge(&mut self) -> Result<TurnResume> {
        let pending = self
            .publication
            .pending
            .as_ref()
            .ok_or_else(|| error::config("turn publication has no pending commit"))?;
        let Response::Committed(descriptor) = self
            .plane
            .turn_checkpoint(self.lease, &pending.request)
            .await?
        else {
            return Err(error::config(
                "turn commit returned another command's response",
            ));
        };
        if *descriptor != pending.expected {
            return Err(error::config(
                "turn commit changed the captured generation or root",
            ));
        }
        self.publication.head = Some((*descriptor).clone());
        self.publication.pending = None;
        Ok(*descriptor)
    }
}
