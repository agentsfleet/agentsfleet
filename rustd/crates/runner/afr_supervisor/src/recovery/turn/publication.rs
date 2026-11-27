//! Keep the exact uncertain commit until a matching receipt or abandonment settles it.

use super::NativeTurn;
use crate::recovery::Lineage;
use crate::{Result, client::TurnCommand, error};
use af_wire::run::{TurnCheckpointAbandon, TurnCheckpointCommit, TurnCheckpointRef, TurnResume};

#[derive(Debug, Default)]
pub(super) struct Publication {
    pub(super) head: Option<TurnResume>,
    pub(super) pending: Option<Pending>,
    /// The tree `head` resolves to, when this run's own save committed it.
    pub(super) lineage: Option<Lineage>,
}

#[derive(Debug)]
pub(super) struct Pending {
    request: TurnCommand<TurnCheckpointCommit>,
    /// The custody the commit names, abandoned if the commit is refused.
    checkpoint: TurnCheckpointRef,
    expected: TurnResume,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.request.zeroize_root();
    }
}

impl NativeTurn<'_> {
    pub(super) async fn reconcile(&mut self) -> Result<()> {
        let Some(pending) = &self.publication.pending else {
            return Ok(());
        };
        let reference = pending.checkpoint.clone();
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
        if self
            .command(TurnCheckpointAbandon {
                checkpoint: reference,
            })
            .await
            .is_ok()
        {
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
        commit: TurnCheckpointCommit,
        expected: TurnResume,
    ) -> Result<TurnResume> {
        let checkpoint = commit.checkpoint.clone();
        self.publication.pending = Some(Pending {
            request: self.request(commit)?,
            checkpoint,
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
        let descriptor = self
            .plane
            .turn_checkpoint(self.run, &pending.request)
            .await?;
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
