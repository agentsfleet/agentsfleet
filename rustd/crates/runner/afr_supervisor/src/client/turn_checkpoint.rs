//! Retain semantic operation identity while each native transmission gets a new request ID.

use std::{borrow::Cow, num::NonZeroU64};

use af_core::id::Uuid7;
use af_wire::{
    protocol::{Down, Up},
    run::{TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointResponse},
};
use backon::Retryable as _;
use garde::Validate as _;

use super::{
    ControlPlane, Verb,
    stream::{Command, Outcome},
};
use crate::{Result, downlink::pending::Expected, error::ErrorKind};

impl ControlPlane {
    pub(crate) async fn turn_checkpoint(
        &self,
        run: &Uuid7,
        request: &TurnCheckpointCommand,
    ) -> Result<TurnCheckpointResponse> {
        request.validate()?;
        let outgoing: Up<'_> = Up::TurnCheckpoint {
            request_id: NonZeroU64::MAX,
            run_id: run.as_str().into(),
            request: Cow::Borrowed(request),
        };
        if !af_validate::serialized_within(&outgoing, af_wire::run::TURN_CHECKPOINT_BYTES) {
            return Err(ErrorKind::StreamRefused {
                code: af_core::error_code::RUN_CHECKPOINT_LIMIT,
                retryable: false,
            }
            .into());
        }
        (|| self.stream.compute(Turn { run, request }))
            .retry(
                self.runtime
                    .backoff()
                    .with_max_times(self.runtime.attempts() - 1),
            )
            .when(crate::Error::is_retryable)
            .await
    }
}

struct Turn<'a> {
    run: &'a Uuid7,
    request: &'a TurnCheckpointCommand,
}

impl Command for Turn<'_> {
    const VERB: Option<Verb> = Some(Verb::TurnCheckpoint);
    type Output = TurnCheckpointResponse;

    fn timeout(&self) -> std::time::Duration {
        if self.request.action.verifies_uploads() {
            TurnCheckpointAction::VERIFY_ANSWER_TIMEOUT
        } else {
            af_core::timing::STREAM_ANSWER_TIMEOUT
        }
    }

    fn expected(&self) -> Expected {
        Expected::TurnCheckpoint {
            run: self.run.clone(),
            operation: self.request.operation_id.clone(),
            kind: self.request.action.kind(),
        }
    }

    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::TurnCheckpoint {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(self.request),
        }
    }

    fn decode(reply: Down<'_>) -> Result<Self::Output> {
        let Down::TurnCheckpoint { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}
