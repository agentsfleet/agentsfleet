//! Offer issue, encoding and delivery each have one ownership boundary.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_observability::verb_span::{OfferStage, VerbClock};
use af_stream::Encoder;
use af_wire::{lease::TaggedLease, protocol::StreamKind};
use afd_fleet::lease::{OfferRequest, Offered};
use tokio::time::Instant;

use super::{Message, SchedulingState, SessionServices};
use crate::{Operation, Result};

impl SessionServices {
    /// Reserve before policy assembly, then transfer the same charged bytes to the writer.
    pub(crate) async fn offer(&self) -> Result<bool> {
        let Some(state) = self.offer_state() else {
            return Ok(false);
        };
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let mut encoder = Encoder::reserve(
            &self.gateway.inner.output_budget,
            StreamKind::Work,
            &self.handle.stop,
            deadline,
        )
        .await?;
        // After the reserve, so output backpressure is not read as claim time.
        let mut clock = VerbClock::<OfferStage>::start();
        // A pass that reaches no stage leaves no span (the clock's own rule):
        // the 5 s probe's empty answer, and a claim that failed before
        // selecting. A claim the gates refused reached a stage and keeps one.
        let Some(offered) = self.issue_offer(&state, &mut encoder, &mut clock).await? else {
            return Ok(false);
        };
        let frame = encoder.finish()?;
        self.cache.insert(offered)?;
        self.handle.snapshot.lock().issued()?;
        let sent = self
            .output
            .send(StreamKind::Work, frame, &self.handle.stop, deadline)
            .await;
        clock.mark(OfferStage::Send);
        sent?;
        Ok(true)
    }

    /// Credits are advisory; the issue transaction still proves physical capacity.
    fn offer_state(&self) -> Option<SchedulingState> {
        let state = self.handle.snapshot.lock();
        (state.ready && !state.degraded && state.credits > 0 && !self.handle.drain.is_cancelled())
            .then(|| state.clone())
    }

    /// The renderer borrows its encoder. No optional writer or ownership extraction is necessary.
    async fn issue_offer(
        &self,
        state: &SchedulingState,
        encoder: &mut Encoder,
        clock: &mut VerbClock<OfferStage>,
    ) -> Result<Option<Offered>> {
        let request = OfferRequest {
            runner: &self.handle.runner,
            held: &state.held,
            degraded: state.degraded,
            now: self.gateway.inner.dependencies.wall_clock.now(),
        };
        let issued = self
            .operation(Operation::Offer, async {
                Ok(self
                    .leases
                    .claim_with(
                        &self.drafts,
                        request,
                        self.accepted,
                        clock,
                        |lease, metadata| encode_offer(encoder, lease, metadata),
                    )
                    .await?)
            })
            .await?;
        issued.transpose()
    }
}

/// Serialization finishes while the policy's borrowed inputs still exist.
fn encode_offer(
    encoder: &mut Encoder,
    lease: TaggedLease<'_, afd_authoring::types::Candidate>,
    offered: Offered,
) -> Result<Offered> {
    serde_json::to_writer(encoder, &Message::Offer { lease })?;
    Ok(offered)
}
