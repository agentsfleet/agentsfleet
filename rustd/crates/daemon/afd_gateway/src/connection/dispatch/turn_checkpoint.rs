//! The physical connection supplies authority; runners never choose event or object namespaces.

use std::num::NonZeroU64;

use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::run::{TurnCheckpointAction, TurnCheckpointCommand};
use afd_fleet::run::turn_checkpoint::{TurnCheckpointRefusal, TurnRun};
use tokio::{sync::SemaphorePermit, time::Instant};

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};

impl ConnectionServices {
    pub(in super::super) async fn turn_checkpoint(
        &self,
        request_id: NonZeroU64,
        run: &str,
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
                let id = Uuid7::parse(run)?;
                let request = garde::Unvalidated::new(request.clone()).validate()?;
                let _permits = bulk.then(|| self.verification_permits()).transpose()?;
                Ok(self
                    .runs
                    .turn_checkpoints
                    .execute(
                        &self.runs.runs,
                        TurnRun {
                            runner_id: &self.handle.runner,
                            run_id: &id,
                            fencing_token: request.fencing_token,
                        },
                        &request,
                    )
                    .await?)
            })
            .await;
        self.send(&Message::TurnCheckpoint {
            request_id,
            run_id: run.into(),
            operation_id: request.operation_id.clone(),
            kind: request.action.kind(),
            answer: answer(result),
        })
        .await
    }

    fn verification_permits(&self) -> Result<(SemaphorePermit<'_>, SemaphorePermit<'_>)> {
        let unavailable = |_closed| afd_fleet::Error::from(TurnCheckpointRefusal::Unavailable);
        let connection = self
            .checkpoint_verifications
            .try_acquire()
            .map_err(unavailable)?;
        let replica = self
            .gateway
            .inner
            .checkpoint_verifications
            .try_acquire()
            .map_err(unavailable)?;
        Ok((connection, replica))
    }
}
