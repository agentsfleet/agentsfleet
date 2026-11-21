//! A run's fenced context verb: its thread's compacted context, a setup
//! draft's or a fleet chat's, read at the run's start and saved after its
//! window compacts.
//!
//! A read is retried like any idempotent verb. A save is retried too: it is a
//! compare-and-swap on the version it names, so a save that landed and lost
//! its answer finds the row already holding its own checkpoint, which the
//! daemon answers as saved.

use std::{borrow::Cow, num::NonZeroU64};

use af_core::id::Uuid7;
use af_wire::{
    context::{ContextLoadRequest, ContextLoadResponse, ContextSaveRequest, ContextSaveResponse},
    protocol::{Down, Up},
};
use backon::Retryable as _;

use super::{
    ControlPlane, Verb,
    stream::{Command, Outcome},
};
use crate::{Result, downlink::pending::Expected, error::ErrorKind};

impl ControlPlane {
    /// The compacted context of the thread `run` runs, read at `fencing_token`.
    ///
    /// # Errors
    /// The daemon refused the read, a superseded run's among them, or never
    /// answered within the runner's retries.
    pub(crate) async fn context_load(
        &self,
        run: &Uuid7,
        fencing_token: u64,
    ) -> Result<ContextLoadResponse> {
        let command = Load {
            run,
            request: ContextLoadRequest { fencing_token },
        };
        (|| self.stream.compute(command.clone()))
            .retry(
                self.runtime
                    .backoff()
                    .with_max_times(self.runtime.attempts() - 1),
            )
            .when(crate::Error::is_retryable)
            .await
    }

    /// Saves `request`'s checkpoint for the thread `run` runs.
    ///
    /// # Errors
    /// The daemon refused the save: a superseded run's, or one another save
    /// moved the row past; or it never answered within the runner's retries.
    pub(crate) async fn context_save(
        &self,
        run: &Uuid7,
        request: &ContextSaveRequest,
    ) -> Result<ContextSaveResponse> {
        (|| self.stream.compute(Save { run, request }))
            .retry(
                self.runtime
                    .backoff()
                    .with_max_times(self.runtime.attempts() - 1),
            )
            .when(crate::Error::is_retryable)
            .await
    }
}

#[derive(Clone)]
struct Load<'a> {
    run: &'a Uuid7,
    request: ContextLoadRequest,
}

impl Command for Load<'_> {
    const VERB: Option<Verb> = Some(Verb::ContextLoad);
    type Output = ContextLoadResponse;

    fn expected(&self) -> Expected {
        Expected::ContextLoad(self.run.clone())
    }

    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::ContextLoad {
            request_id,
            run_id: self.run.as_str().into(),
            request: self.request,
        }
    }

    fn decode(reply: Down<'_>) -> Result<Self::Output> {
        let Down::ContextLoaded { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}

struct Save<'a> {
    run: &'a Uuid7,
    request: &'a ContextSaveRequest,
}

impl Command for Save<'_> {
    const VERB: Option<Verb> = Some(Verb::ContextSave);
    type Output = ContextSaveResponse;

    fn expected(&self) -> Expected {
        Expected::ContextSave(self.run.clone())
    }

    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::ContextSave {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(self.request),
        }
    }

    fn decode(reply: Down<'_>) -> Result<Self::Output> {
        let Down::ContextSaved { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}
