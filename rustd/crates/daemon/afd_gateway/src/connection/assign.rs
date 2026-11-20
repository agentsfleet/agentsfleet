//! Assignment issue, encoding and delivery each have one ownership boundary.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_observability::verb_span::{AssignStage, VerbClock};
use af_stream::Encoder;
use af_wire::{protocol::StreamKind, run::TaggedRun};
use afd_fleet::run::{AssignRequest, AssignedRun, RecoveryScan};
use tokio::{sync::oneshot, time::Instant};

use super::unframed::{Cause, Unsent};
use super::{
    AssignMode, AssignOutcome, ConnectionServices, Message, SchedulingState, io::StreamWriters,
};
use crate::{Operation, Result};

/// What encoding an assignment came to: a frame ready to send, or a run its
/// frame cannot carry, which is failed instead of sent.
#[derive(Debug)]
enum Encoded {
    Framed {
        assigned: Box<AssignedRun>,
        unsent: Unsent,
    },
    Unframed(Unsent),
}

impl ConnectionServices {
    /// Serialize inside the assignment, charge the frame its exact length, then
    /// hand the same charged bytes to the writer.
    pub(crate) async fn assign(&self, mode: AssignMode) -> Result<AssignOutcome> {
        let Some(state) = self.assign_state() else {
            return Ok(AssignOutcome::Empty);
        };
        let mut encoder = Encoder::new(&self.gateway.inner.output_budget, StreamKind::Work);
        let mut clock = VerbClock::<AssignStage>::start();
        // A pass that reaches no stage leaves no span (the clock's own rule):
        // the 5 s probe's empty answer, and an assignment that failed before
        // selecting. An assignment the gates refused reached a stage and keeps one.
        let (assigned, deferred) = self
            .select_assignment(mode, &state, &mut encoder, &mut clock)
            .await?;
        let Some(issued) = assigned else {
            return Ok(if deferred {
                AssignOutcome::Deferred
            } else {
                AssignOutcome::Empty
            });
        };
        let (assigned, unsent) = match issued {
            Encoded::Framed { assigned, unsent } => (*assigned, unsent),
            // Failed rather than sent: nothing reached the runner, so its
            // capacity is untouched and the next pass assigns afresh.
            Encoded::Unframed(unsent) => {
                self.not_sent(&unsent, Cause::TooLarge).await?;
                return Ok(AssignOutcome::Empty);
            }
        };
        // The run is committed. A frame that never reaches the writer releases
        // its run back to the queue rather than leave it to lapse on a runner
        // that never heard of it.
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let run = assigned.run_id.clone();
        let handed = self.hand_over(encoder, assigned, deadline).await;
        let sent = match handed {
            Ok(flushed) => StreamWriters::flushed(flushed, &self.handle.stop, deadline).await,
            Err(error) => {
                self.cache.forget(&run);
                if let Err(release) = self.not_sent(&unsent, Cause::Undelivered).await {
                    crate::Gateway::observe(&release, Operation::Assign);
                }
                Err(error)
            }
        };
        clock.mark(AssignStage::Send);
        sent?;
        Ok(AssignOutcome::Issued)
    }

    /// Charges the frame, records the run as this runner's, and hands the
    /// frame to the writer. A refusal here means no byte of it left.
    async fn hand_over(
        &self,
        encoder: Encoder,
        assigned: AssignedRun,
        deadline: Instant,
    ) -> Result<oneshot::Receiver<()>> {
        let frame = encoder.finish(&self.handle.stop, deadline).await?;
        self.cache
            .insert(assigned, self.gateway.inner.dependencies.wall_clock.now())?;
        self.handle.snapshot.lock().issued()?;
        self.output
            .enqueue(StreamKind::Work, frame, &self.handle.stop, deadline)
            .await
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
    ) -> Result<(Option<Encoded>, bool)> {
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
    ) -> Result<Option<Encoded>> {
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
///
/// Measured first: a run past the frame is named for failing before a byte
/// reaches the encoder, which would refuse it part-way through.
fn encode_assignment(
    encoder: &mut Encoder,
    run: TaggedRun<'_>,
    assigned: AssignedRun,
) -> Result<Encoded> {
    let unsent = Unsent::of(&run);
    let message = Message::Run { run };
    if !Unsent::fits(&message) {
        return Ok(Encoded::Unframed(unsent));
    }
    encoder.frame(&message)?;
    Ok(Encoded::Framed {
        assigned: Box::new(assigned),
        unsent,
    })
}
