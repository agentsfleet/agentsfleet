//! Ordinary roots use native event-owned commands, without a synthetic task identity.

mod capture;
mod publication;
mod reading;
#[cfg(test)]
mod reading_tests;
mod restore;
mod session;

use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::run::{
    CheckpointOperationId, RunPayload, TurnCheckpointAdopt, TurnCheckpointPinRelease,
    TurnCheckpointRef, TurnCheckpointRequest, TurnCheckpointSaved, TurnWorkspace,
};
use afr_agent::TurnRecovery;
use zeroize::Zeroizing;

use crate::{
    Result,
    client::{ControlPlane, TurnCommand},
    error,
};

#[derive(Debug)]
pub(crate) struct NativeTurn<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) run: &'a Uuid7,
    pub(crate) fence: u64,
    transfers: &'a super::Transfers,
    publication: publication::Publication,
}

impl<'a> NativeTurn<'a> {
    pub(crate) fn new(
        plane: &'a ControlPlane,
        run: &'a Uuid7,
        fence: u64,
        transfers: &'a super::Transfers,
    ) -> Self {
        Self {
            plane,
            run,
            fence,
            transfers,
            publication: publication::Publication::default(),
        }
    }

    /// The generation a pin may read only what changed since: the head, when
    /// this run's own save committed it and holds the tree it resolves to.
    pub(crate) fn parent(&self) -> Option<&Uuid7> {
        let lineage = self.publication.lineage.as_ref()?;
        let head = self.publication.head.as_ref()?;
        let head = Uuid7::parse(&head.checkpoint_id.0).ok()?;
        (*lineage.head() == head).then(|| lineage.head())
    }

    fn request<R: TurnCheckpointRequest>(&self, request: R) -> Result<TurnCommand<R>> {
        Ok(TurnCommand::new(
            self.fence,
            CheckpointOperationId(Entropy::new().uuid7(clock::now())?.to_string()),
            request,
        ))
    }

    /// Sends `request` under a fresh operation and answers its own response.
    pub(crate) async fn command<R: TurnCheckpointRequest>(
        &self,
        request: R,
    ) -> Result<R::Response> {
        let command = self.request(request)?;
        self.plane.turn_checkpoint(self.run, &command).await
    }

    async fn adopt(
        &self,
        assignment: &RunPayload<'_>,
        saved: TurnCheckpointSaved,
        workspace: TurnWorkspace,
    ) -> Result<TurnRecovery> {
        let reference = TurnCheckpointRef {
            checkpoint_id: saved.descriptor.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(saved.pin.id),
        };
        let result = async {
            let mut recovered = TurnRecovery {
                descriptor: saved.descriptor,
                history: Zeroizing::new(saved.root.history.into_bytes()),
                adopted: saved.folds,
                workspace,
            };
            recovered.validate_for(assignment).map_err(|source| {
                error::boundary("ordinary root validation refused restore", source)
            })?;
            let adopted = self
                .command(TurnCheckpointAdopt {
                    checkpoint: reference.clone(),
                })
                .await?;
            if adopted != recovered.adopted {
                return Err(error::config(
                    "turn adoption changed saved input membership",
                ));
            }
            recovered.adopted = adopted;
            Ok(recovered)
        }
        .await;
        let released = self
            .command(TurnCheckpointPinRelease {
                checkpoint: reference,
            })
            .await;
        match (result, released) {
            (Ok(recovered), Ok(())) => Ok(recovered),
            (Err(failure), _) | (Ok(_), Err(failure)) => Err(failure),
        }
    }
}
