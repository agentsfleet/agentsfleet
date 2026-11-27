//! Retain semantic operation identity while each native transmission gets a new request ID.

use std::{borrow::Cow, marker::PhantomData, num::NonZeroU64};

use af_core::id::Uuid7;
use af_stream::OwnedFrame;
use af_wire::{
    protocol::Up,
    run::{
        CheckpointOperationId, TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointCommit,
        TurnCheckpointRequest, TurnCheckpointResponse,
    },
};
use backon::Retryable as _;
use garde::Validate as _;
use zeroize::Zeroize as _;

use super::{
    ControlPlane, Verb,
    stream::{Answered, Command, Outcome},
};
use crate::{Result, downlink::pending::Expected, error::ErrorKind};

/// A checkpoint command built from one `R`, so its answer is `R::Response`.
///
/// Retries resend these exact bytes under fresh transport identifiers.
#[derive(Debug)]
pub(crate) struct TurnCommand<R> {
    command: TurnCheckpointCommand,
    request: PhantomData<fn() -> R>,
}

impl<R: TurnCheckpointRequest> TurnCommand<R> {
    pub(crate) fn new(fencing_token: u64, operation_id: CheckpointOperationId, request: R) -> Self {
        Self {
            command: TurnCheckpointCommand {
                fencing_token,
                operation_id,
                action: request.into(),
            },
            request: PhantomData,
        }
    }
}

impl TurnCommand<TurnCheckpointCommit> {
    /// Wipes the private root a commit carries once it is no longer resent.
    pub(crate) fn zeroize_root(&mut self) {
        if let TurnCheckpointAction::Commit(commit) = &mut self.command.action {
            commit.root.history.zeroize();
        }
    }
}

impl ControlPlane {
    pub(crate) async fn turn_checkpoint<R: TurnCheckpointRequest>(
        &self,
        run: &Uuid7,
        command: &TurnCommand<R>,
    ) -> Result<R::Response> {
        command.command.validate()?;
        let outgoing: Up<'_> = Up::TurnCheckpoint {
            request_id: NonZeroU64::MAX,
            run_id: run.as_str().into(),
            request: Cow::Borrowed(&command.command),
        };
        if !af_validate::serialized_within(&outgoing, af_wire::protocol::MESSAGE_BYTES_MAX) {
            return Err(ErrorKind::StreamRefused {
                code: af_core::error_code::RUN_CHECKPOINT_LIMIT,
                retryable: false,
            }
            .into());
        }
        (|| self.stream.compute(Turn { run, command }))
            .retry(
                self.runtime
                    .backoff()
                    .with_max_times(self.runtime.attempts() - 1),
            )
            .when(crate::Error::is_retryable)
            .await
    }
}

struct Turn<'a, R> {
    run: &'a Uuid7,
    command: &'a TurnCommand<R>,
}

impl<R: TurnCheckpointRequest> Command for Turn<'_, R> {
    const VERB: Option<Verb> = Some(Verb::TurnCheckpoint);
    type Output = R::Response;

    fn expected(&self) -> Expected {
        Expected::TurnCheckpoint {
            run: self.run.clone(),
            operation: self.command.command.operation_id.clone(),
            kind: self.command.command.action.kind(),
        }
    }

    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::TurnCheckpoint {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(&self.command.command),
        }
    }

    /// Correlation admitted only the variant answering this kind.
    fn decode(reply: &OwnedFrame) -> Result<Self::Output> {
        let Answered { answer } = reply.decode::<Answered<'_, TurnCheckpointResponse>>()?;
        R::answer(Outcome::try_from(answer)?.accepted()?)
            .ok_or_else(|| ErrorKind::StreamProtocol.into())
    }
}
