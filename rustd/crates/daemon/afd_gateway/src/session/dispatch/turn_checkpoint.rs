//! The physical session supplies authority; runners never choose event or object namespaces.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::lease::TurnCheckpointCommand;
use afd_fleet::lease::turn_checkpoint::TurnLease;

use super::super::{Message, SessionServices, context::answer};
use crate::{Operation, Result};

impl SessionServices {
    pub(in super::super) async fn turn_checkpoint(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: &TurnCheckpointCommand,
    ) -> Result<()> {
        let result = self
            .operation(Operation::TurnCheckpoint, async {
                let id = Uuid7::parse(lease)?;
                let request = garde::Unvalidated::new(request.clone()).validate()?;
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
}
