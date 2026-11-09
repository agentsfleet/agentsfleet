//! Offer issue, encoding and delivery each have one ownership boundary.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Encoder;
use af_wire::{lease::TaggedLease, protocol::StreamKind};
use afd_fleet::lease::{OfferRequest, Offered};
use tokio::time::Instant;

use crate::{Operation, Result};
use super::{Message, SchedulingState, SessionServices};

impl SessionServices {
    /// Reserve before policy assembly, then transfer the same charged bytes to the writer.
    pub(super) async fn offer(&self) -> Result<()> {
        let Some(state) = self.offer_state() else { return Ok(()); };
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let mut encoder = Encoder::reserve(&self.gateway.0.output_budget, StreamKind::Work,
            &self.handle.stop, deadline).await?;
        let Some(offered) = self.issue_offer(&state, &mut encoder).await? else { return Ok(()); };
        let frame = encoder.finish()?;
        self.cache.insert(offered)?;
        self.handle.snapshot.send_modify(|state| state.credits = state.credits.saturating_sub(1));
        self.output.send(StreamKind::Work, frame, &self.handle.stop, deadline).await?;
        // Continue filling real idle capacity; an empty claim stops this notification chain.
        self.handle.request_offer();
        Ok(())
    }

    /// Credits are advisory; the issue transaction still proves physical capacity.
    fn offer_state(&self) -> Option<SchedulingState> {
        let state = self.handle.snapshot.borrow();
        (state.ready && !state.degraded && state.credits > 0 && !self.handle.drain.is_cancelled())
            .then(|| state.clone())
    }

    /// The renderer borrows its encoder. No optional writer or ownership extraction is necessary.
    async fn issue_offer(&self, state: &SchedulingState, encoder: &mut Encoder) -> Result<Option<Offered>> {
        let request = OfferRequest {
            runner: &self.handle.runner, held: &state.held, degraded: state.degraded,
            now: self.gateway.0.dependencies.wall_clock.now(),
        };
        let issued = self.operation(Operation::Offer, async {
            Ok(self.leases.claim_with(&self.drafts, request, self.accepted,
                |lease, metadata| encode_offer(encoder, lease, metadata)).await?)
        }).await?;
        issued.transpose()
    }
}

/// Serialization finishes while the policy's borrowed inputs still exist.
fn encode_offer(
    encoder: &mut Encoder, lease: TaggedLease<'_, afd_authoring::types::Candidate>, offered: Offered,
) -> Result<Offered> {
    serde_json::to_writer(encoder, &Message::Offer { lease })?;
    Ok(offered)
}
