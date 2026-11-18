//! Ordinary roots use native event-owned commands, without a synthetic task identity.

mod capture;
mod files;
#[cfg(test)]
mod files_tests;
mod publication;
mod restore;
mod session;

use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::lease::{
    LeasePayload, TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointRead,
    TurnCheckpointRef, TurnCheckpointResponse as Response, TurnId, TurnWorkspace,
};
use afr_agent::TurnRecovery;
use zeroize::Zeroizing;

use crate::{Result, client::ControlPlane, error};

#[derive(Debug)]
pub(crate) struct NativeTurn<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) lease: &'a Uuid7,
    pub(crate) fence: u64,
    transfers: &'a super::Transfers,
    publication: publication::Publication,
}

impl<'a> NativeTurn<'a> {
    pub(crate) fn new(
        plane: &'a ControlPlane,
        lease: &'a Uuid7,
        fence: u64,
        transfers: &'a super::Transfers,
    ) -> Self {
        Self {
            plane,
            lease,
            fence,
            transfers,
            publication: publication::Publication::default(),
        }
    }

    fn request(&self, action: Action) -> Result<TurnCheckpointCommand> {
        Ok(TurnCheckpointCommand {
            fencing_token: self.fence,
            operation_id: TurnId(Entropy::new().uuid7(clock::now())?.to_string()),
            action,
        })
    }

    pub(crate) async fn command(&self, action: Action) -> Result<Response> {
        let request = self.request(action)?;
        self.plane.turn_checkpoint(self.lease, &request).await
    }

    async fn adopt(
        &self,
        offer: &LeasePayload<'_>,
        saved: TurnCheckpointRead,
        workspace: TurnWorkspace,
    ) -> Result<TurnRecovery> {
        let reference = TurnCheckpointRef {
            checkpoint_id: saved.descriptor.checkpoint_id.clone(),
            pin_id: TurnId(saved.pin.id),
        };
        let result = async {
            let mut recovered = TurnRecovery {
                descriptor: saved.descriptor,
                history: Zeroizing::new(saved.root.history.into_bytes()),
                adopted: saved.folds,
                workspace,
            };
            recovered.validate_for(offer).map_err(|source| {
                error::boundary("ordinary root validation refused restore", source)
            })?;
            let Response::Adopted(adopted) = self
                .command(Action::Adopt {
                    checkpoint: reference.clone(),
                })
                .await?
            else {
                return Err(error::config(
                    "turn adoption returned another command's response",
                ));
            };
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
            .command(Action::PinRelease {
                checkpoint: reference,
            })
            .await;
        match (result, released) {
            (Ok(recovered), Ok(Response::PinReleased)) => Ok(recovered),
            (Err(failure), _) | (Ok(_), Err(failure)) => Err(failure),
            (Ok(_), Ok(_)) => Err(error::config(
                "turn release returned another command's response",
            )),
        }
    }
}
