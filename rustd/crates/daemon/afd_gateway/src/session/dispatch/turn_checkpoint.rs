//! The physical session supplies authority; runners never choose event or object namespaces.

use std::num::NonZeroU64;

use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::lease::{TurnCheckpointAction, TurnCheckpointCommand};
use afd_fleet::lease::turn_checkpoint::{TurnCheckpointRefusal, TurnLease};
use tokio::{sync::SemaphorePermit, time::Instant};

use super::super::{Message, SessionServices, context::answer};
use crate::{Operation, Result};

impl SessionServices {
    pub(in super::super) async fn turn_checkpoint(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: &TurnCheckpointCommand,
    ) -> Result<()> {
        let bulk = request.action.verifies_uploads();
        // Preserve a regular answer budget on either side of the slow operation.
        let work = if bulk {
            TurnCheckpointAction::VERIFY_ANSWER_TIMEOUT
                .saturating_sub(STREAM_ANSWER_TIMEOUT.saturating_mul(2))
        } else {
            STREAM_ANSWER_TIMEOUT
        };
        let result = self
            .operation_until(Operation::TurnCheckpoint, Instant::now() + work, async {
                let id = Uuid7::parse(lease)?;
                let request = garde::Unvalidated::new(request.clone()).validate()?;
                let _permits = bulk.then(|| self.verification_permits()).transpose()?;
                Ok(self
                    .leases
                    .turn_checkpoints
                    .execute(
                        &self.leases.leases,
                        TurnLease {
                            runner_id: &self.handle.runner,
                            lease_id: &id,
                            fencing_token: request.fencing_token,
                        },
                        &request,
                    )
                    .await?)
            })
            .await;
        self.send(&Message::TurnCheckpoint {
            request_id,
            lease_id: lease.into(),
            operation_id: request.operation_id.clone(),
            kind: request.action.kind(),
            answer: answer(result),
        })
        .await
    }

    fn verification_permits(&self) -> Result<(SemaphorePermit<'_>, SemaphorePermit<'_>)> {
        let unavailable = |_closed| afd_fleet::Error::from(TurnCheckpointRefusal::Unavailable);
        let session = self
            .checkpoint_verifications
            .try_acquire()
            .map_err(unavailable)?;
        let replica = self
            .gateway
            .inner
            .checkpoint_verifications
            .try_acquire()
            .map_err(unavailable)?;
        Ok((session, replica))
    }
}
