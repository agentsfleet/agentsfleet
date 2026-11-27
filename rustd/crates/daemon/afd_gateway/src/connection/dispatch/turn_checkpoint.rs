//! The physical connection supplies authority; runners never choose event or object namespaces.

use std::num::NonZeroU64;

use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::run::TurnCheckpointCommand;
use afd_fleet::run::turn_checkpoint::TurnRun;
use tokio::time::Instant;

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};

impl ConnectionServices {
    pub(in super::super) async fn turn_checkpoint(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: &TurnCheckpointCommand,
    ) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let result = self
            .operation_until(Operation::TurnCheckpoint, deadline, async {
                let id = Uuid7::parse(run)?;
                let request = garde::Unvalidated::new(request.clone()).validate()?;
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
}
