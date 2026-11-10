//! A replica pass awaits one claim at a time through the session's existing operation owner.

use super::SessionHandle;
use crate::{
    Operation, Result,
    error::{closed_send, within},
};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use tokio::{sync::oneshot, time::Instant};

pub(super) enum OfferTrigger {
    Ready,
    Pass(oneshot::Sender<bool>),
}

impl SessionHandle {
    pub(crate) async fn claim(&self) -> Result<bool> {
        if self.stop.is_cancelled() || self.drain.is_cancelled() || self.superseded.is_cancelled() {
            return Ok(false);
        }
        let (reply, answer) = oneshot::channel();
        within(
            &self.stop,
            Instant::now() + STREAM_ANSWER_TIMEOUT,
            Operation::Offer,
            async {
                self.claims.send(reply).await.map_err(closed_send)?;
                Ok(answer.await?)
            },
        )
        .await
    }
}
