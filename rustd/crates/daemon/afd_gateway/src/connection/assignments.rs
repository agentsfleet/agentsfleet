//! A replica pass awaits one assignment at a time through the connection's operation owner.

use super::ConnectionHandle;
use crate::{
    Operation, Result,
    error::{closed_send, within},
};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use tokio::{sync::oneshot, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignMode {
    Ordinary,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignOutcome {
    Issued,
    Empty,
    Deferred,
}

#[derive(Debug)]
pub(crate) struct PassRequest {
    pub(crate) mode: AssignMode,
    pub(crate) reply: oneshot::Sender<AssignOutcome>,
}

pub(super) enum AssignTrigger {
    Ready,
    Pass(PassRequest),
}

impl ConnectionHandle {
    pub(crate) async fn assign(&self, mode: AssignMode) -> Result<AssignOutcome> {
        if self.stop.is_cancelled() || self.drain.is_cancelled() || self.superseded.is_cancelled() {
            return Ok(AssignOutcome::Empty);
        }
        let (reply, answer) = oneshot::channel();
        within(
            &self.stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
            Operation::Assign,
            async {
                self.passes
                    .send(PassRequest { mode, reply })
                    .await
                    .map_err(closed_send)?;
                Ok(answer.await?)
            },
        )
        .await
    }
}
