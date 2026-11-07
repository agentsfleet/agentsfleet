//! Summary status uses the same creator/lease guard as private answer previews.

use afd_core::id::Uuid7;
use afd_dragonfly::FleetStreams;
use afd_wire::context::{ContextProgress, ContextState};
use serde::{Deserialize, Serialize};

use super::{
    draft_channel,
    publish::{DEADLINE, authorize},
    trace::Trace,
};
use crate::{Drafts, lease::guard};

/// Creator-private metadata; generated summary text never enters this channel.
pub const CONTEXT_EVENT: &str = "context.progress";

/// One bounded status hint; reconnecting readers clear it and reload message state.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextNotice {
    /// The authenticated draft subject.
    #[garde(skip)]
    pub draft_id: Uuid7,
    /// The currently running user message.
    #[garde(length(min = 1, max = 19), custom(afd_validate::ascii_digits))]
    pub sequence: String,
    /// The physical summary dispatch, independent of a subsequent ordinary call.
    #[garde(skip)]
    pub stream_id: Uuid7,
    /// Counts and closed state only.
    #[garde(dive)]
    pub progress: ContextProgress,
}

pub(in crate::model) struct ContextPublisher<'a> {
    streams: &'a FleetStreams,
    drafts: &'a Drafts,
    runner: &'a Uuid7,
    lease: &'a Uuid7,
    counters: &'a guard::Counters,
    dispatch: Uuid7,
    generation: u64,
}

impl<'a> ContextPublisher<'a> {
    pub(in crate::model) fn new(
        streams: &'a FleetStreams,
        drafts: &'a Drafts,
        runner: &'a Uuid7,
        lease: &'a Uuid7,
        counters: &'a guard::Counters,
        dispatch: Uuid7,
        generation: u64,
    ) -> Self {
        Self {
            streams,
            drafts,
            runner,
            lease,
            counters,
            dispatch,
            generation,
        }
    }

    pub(in crate::model) async fn send(&self, state: ContextState) {
        let mut trace = Trace::start(self.dispatch.clone());
        match tokio::time::timeout(DEADLINE, self.publish(state)).await {
            Ok(Ok(())) => {
                trace.published();
                trace.completed();
            }
            Ok(Err(error)) => trace.failed(error.code(), "context_publication_refused"),
            Err(_) => trace.failed(afd_core::error_code::INTERNAL_OPERATION_FAILED, "deadline"),
        }
    }

    async fn publish(&self, state: ContextState) -> crate::Result<()> {
        let (draft_id, sequence) =
            authorize(self.drafts, self.runner, self.lease, self.counters).await?;
        let frame = ContextNotice {
            draft_id,
            sequence: sequence.to_string(),
            stream_id: self.dispatch.clone(),
            progress: ContextProgress {
                generation: self.generation,
                state,
            },
        };
        let payload = serde_json::to_string(&frame).map_err(crate::error::raise::stored)?;
        self.streams
            .publish(&draft_channel(&frame.draft_id), &payload)
            .await
            .map(|_subscribers| ())
            .map_err(|source| crate::error::ErrorKind::Preview { source }.into())
    }
}
