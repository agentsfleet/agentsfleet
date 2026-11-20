//! Assignment issue, encoding and delivery each have one ownership boundary.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_observability::verb_span::{AssignStage, VerbClock};
use af_stream::Encoder;
use af_wire::{protocol::StreamKind, run::TaggedRun};
use afd_fleet::run::{AssignRequest, AssignedRun, RecoveryScan};
use tokio::time::Instant;

use super::{AssignMode, AssignOutcome, ConnectionServices, Message, SchedulingState};
use crate::{Operation, Result};

impl ConnectionServices {
    /// Reserve before policy assembly, then transfer the same charged bytes to the writer.
    pub(crate) async fn assign(&self, mode: AssignMode) -> Result<AssignOutcome> {
        let Some(state) = self.assign_state() else {
            return Ok(AssignOutcome::Empty);
        };
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let mut encoder = Encoder::reserve(
            &self.gateway.inner.output_budget,
            StreamKind::Work,
            &self.handle.stop,
            deadline,
        )
        .await?;
        // After the reserve, so output backpressure is not read as selection time.
        let mut clock = VerbClock::<AssignStage>::start();
        // A pass that reaches no stage leaves no span (the clock's own rule):
        // the 5 s probe's empty answer, and an assignment that failed before
        // selecting. An assignment the gates refused reached a stage and keeps one.
        let (assigned, deferred) = self
            .select_assignment(mode, &state, &mut encoder, &mut clock)
            .await?;
        let Some(assigned) = assigned else {
            return Ok(if deferred {
                AssignOutcome::Deferred
            } else {
                AssignOutcome::Empty
            });
        };
        let frame = encoder.finish()?;
        self.cache
            .insert(assigned, self.gateway.inner.dependencies.wall_clock.now())?;
        self.handle.snapshot.lock().issued()?;
        let sent = self
            .output
            .send(StreamKind::Work, frame, &self.handle.stop, deadline)
            .await;
        clock.mark(AssignStage::Send);
        sent?;
        Ok(AssignOutcome::Issued)
    }

    /// Credits are advisory; the issue transaction still proves physical capacity.
    fn assign_state(&self) -> Option<SchedulingState> {
        let state = self.handle.snapshot.lock();
        (state.ready && !state.degraded && state.credits > 0 && !self.handle.drain.is_cancelled())
            .then(|| state.clone())
    }

    async fn select_assignment(
        &self,
        mode: AssignMode,
        state: &SchedulingState,
        encoder: &mut Encoder,
        clock: &mut VerbClock<AssignStage>,
    ) -> Result<(Option<AssignedRun>, bool)> {
        let mut scan = (mode == AssignMode::Recovery)
            .then(|| std::mem::take(&mut *self.handle.recovery_scan.lock()));
        if let Some(scan) = scan.as_mut() {
            scan.begin_attempt();
        }
        let assigned = self
            .issue_assignment(state, encoder, clock, scan.as_mut())
            .await;
        let deferred = scan.is_some_and(|scan| {
            let deferred = scan.deferred();
            *self.handle.recovery_scan.lock() = scan;
            deferred
        });
        let assigned = assigned?;
        if mode == AssignMode::Recovery && assigned.is_none() && !deferred {
            self.handle
                .snapshot
                .lock()
                .recovered(state.recovery_pending);
        }
        Ok((assigned, deferred))
    }

    /// The renderer borrows its encoder. No optional writer or ownership extraction is necessary.
    async fn issue_assignment(
        &self,
        state: &SchedulingState,
        encoder: &mut Encoder,
        clock: &mut VerbClock<AssignStage>,
        recovery: Option<&mut RecoveryScan>,
    ) -> Result<Option<AssignedRun>> {
        let request = AssignRequest {
            runner: &self.handle.runner,
            held: &state.held,
            degraded: state.degraded,
            now: self.gateway.inner.dependencies.wall_clock.now(),
            recovery,
        };
        let issued = self
            .operation(Operation::Assign, async {
                Ok(self
                    .runs
                    .assign_with(
                        &self.drafts,
                        request,
                        self.accepted,
                        self.draft_runs,
                        clock,
                        |run, metadata| encode_assignment(encoder, run, metadata),
                    )
                    .await?)
            })
            .await?;
        issued.transpose()
    }
}

/// Serialization finishes while the policy's borrowed inputs still exist.
fn encode_assignment(
    encoder: &mut Encoder,
    run: TaggedRun<'_, afd_setup::types::Candidate>,
    assigned: AssignedRun,
) -> Result<AssignedRun> {
    serde_json::to_writer(encoder, &Message::Run { run })?;
    Ok(assigned)
}
