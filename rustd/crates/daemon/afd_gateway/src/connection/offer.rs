//! Offer issue, encoding and delivery each have one ownership boundary.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_observability::verb_span::{OfferStage, VerbClock};
use af_stream::Encoder;
use af_wire::{protocol::StreamKind, run::TaggedRun};
use afd_fleet::run::{OfferRequest, Offered, RecoveryScan};
use tokio::time::Instant;

use super::{ConnectionServices, Message, OfferMode, OfferOutcome, SchedulingState};
use crate::{Operation, Result};

impl ConnectionServices {
    /// Reserve before policy assembly, then transfer the same charged bytes to the writer.
    pub(crate) async fn offer(&self, mode: OfferMode) -> Result<OfferOutcome> {
        let Some(state) = self.offer_state() else {
            return Ok(OfferOutcome::Empty);
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
        let (offered, deferred) = self
            .select_offer(mode, &state, &mut encoder, &mut clock)
            .await?;
        let Some(offered) = offered else {
            return Ok(if deferred {
                OfferOutcome::Deferred
            } else {
                OfferOutcome::Empty
            });
        };
        let frame = encoder.finish()?;
        self.cache
            .insert(offered, self.gateway.inner.dependencies.wall_clock.now())?;
        self.handle.snapshot.lock().issued()?;
        let sent = self
            .output
            .send(StreamKind::Work, frame, &self.handle.stop, deadline)
            .await;
        clock.mark(OfferStage::Send);
        sent?;
        Ok(OfferOutcome::Issued)
    }

    /// Credits are advisory; the issue transaction still proves physical capacity.
    fn offer_state(&self) -> Option<SchedulingState> {
        let state = self.handle.snapshot.lock();
        (state.ready && !state.degraded && state.credits > 0 && !self.handle.drain.is_cancelled())
            .then(|| state.clone())
    }

    async fn select_offer(
        &self,
        mode: OfferMode,
        state: &SchedulingState,
        encoder: &mut Encoder,
        clock: &mut VerbClock<OfferStage>,
    ) -> Result<(Option<Offered>, bool)> {
        let mut scan = (mode == OfferMode::Recovery)
            .then(|| std::mem::take(&mut *self.handle.recovery_scan.lock()));
        if let Some(scan) = scan.as_mut() {
            scan.begin_attempt();
        }
        let offered = self.issue_offer(state, encoder, clock, scan.as_mut()).await;
        let deferred = scan.is_some_and(|scan| {
            let deferred = scan.deferred();
            *self.handle.recovery_scan.lock() = scan;
            deferred
        });
        let offered = offered?;
        if mode == OfferMode::Recovery && offered.is_none() && !deferred {
            self.handle
                .snapshot
                .lock()
                .recovered(state.recovery_pending);
        }
        Ok((offered, deferred))
    }

    /// The renderer borrows its encoder. No optional writer or ownership extraction is necessary.
    async fn issue_offer(
        &self,
        state: &SchedulingState,
        encoder: &mut Encoder,
        clock: &mut VerbClock<OfferStage>,
        recovery: Option<&mut RecoveryScan>,
    ) -> Result<Option<Offered>> {
        let request = OfferRequest {
            runner: &self.handle.runner,
            held: &state.held,
            degraded: state.degraded,
            now: self.gateway.inner.dependencies.wall_clock.now(),
            recovery,
        };
        let issued = self
            .operation(Operation::Offer, async {
                Ok(self
                    .runs
                    .claim_with(
                        &self.drafts,
                        request,
                        self.accepted,
                        self.draft_runs,
                        clock,
                        |run, metadata| encode_offer(encoder, run, metadata),
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
    run: TaggedRun<'_, afd_setup::types::Candidate>,
    offered: Offered,
) -> Result<Offered> {
    serde_json::to_writer(encoder, &Message::Offer { run })?;
    Ok(offered)
}
