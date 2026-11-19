//! A replica pass awaits one claim at a time through the session's existing operation owner.

use super::SessionHandle;
use crate::{
    Operation, Result,
    error::{closed_send, within},
};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use tokio::{sync::oneshot, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfferMode {
    Ordinary,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfferOutcome {
    Issued,
    Empty,
    Deferred,
}

#[derive(Debug)]
pub(crate) struct ClaimRequest {
    pub(crate) mode: OfferMode,
    pub(crate) reply: oneshot::Sender<OfferOutcome>,
}

pub(super) enum OfferTrigger {
    Ready,
    Pass(ClaimRequest),
}

impl SessionHandle {
    pub(crate) async fn claim(&self, mode: OfferMode) -> Result<OfferOutcome> {
        if self.stop.is_cancelled() || self.drain.is_cancelled() || self.superseded.is_cancelled() {
            return Ok(OfferOutcome::Empty);
        }
        let (reply, answer) = oneshot::channel();
        within(
            &self.stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
            Operation::Offer,
            async {
                self.claims
                    .send(ClaimRequest { mode, reply })
                    .await
                    .map_err(closed_send)?;
                Ok(answer.await?)
            },
        )
        .await
    }
}
