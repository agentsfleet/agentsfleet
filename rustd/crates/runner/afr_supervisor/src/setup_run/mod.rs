//! A setup draft's turn on the common run path: a run the draft owns, with
//! no sandbox, `propose_setup` alone, and a report carrying the proposal the
//! run kept.
//!
//! ```text
//!  Down::Run { owner: draft } ─► occupy a worker ─► renew alongside ─► live answer pump
//!    ─► agent.setup: the guffbolt loop, its context through the fenced verb
//!    ─► report: the answer, the usage, ReportRequest.setup { the kept proposal }
//!    ─► acknowledged; the pump drains within its bound
//! ```
//!
//! It runs as a fleet run does (`crate::run_loop`) where the two share a
//! step: renewal, the live tail, a Stop, a cut and the report's delivery. It
//! skips what a setup run has none of: an egress plan, a sandbox and its
//! hold, memory, recovery and tool records. The daemon checks the proposal
//! again in the report's transaction before it writes the next candidate.

mod context;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use af_core::error_code::{self, Coded as _};
use af_core::id::Uuid7;
use af_wire::report::FailureClass;
use af_wire::run::{RunPayload, SetupProposal, SetupReport};
use afr_agent::{Meter, RunOutput, SetupOutput, SetupRun};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use zeroize::Zeroizing;

use self::context::RunContext;
use crate::activity::{self, ActivitySink};
use crate::error::{self, Result};
use crate::holds::Occupant;
use crate::identity::run_span;
use crate::renew::{self, Renewal};
use crate::report::{Ending, delivery::until_acknowledged, report};
use crate::run_loop::{ACTIVITY_DRAIN_WAIT, Lessee};

const DETAIL_ENGINE: &str = "the setup run could not start its model";
const DETAIL_RENEWAL: &str = "the daemon ended the run while it ran";
const DETAIL_STOPPED: &str = "this runner was told to stop while the run went on";
const EVENT_ACQUIRED: &str = "setup_run_acquired";
const EVENT_ENGINE_FAILED: &str = "setup_run_engine_failed";
const EVENT_REPORTED: &str = "setup_run_reported";
const EVENT_DRAIN_ABANDONED: &str = "activity_drain_abandoned";

/// What the run's work came to: how it ended, and the proposal it kept.
struct Worked {
    ending: Ending,
    proposal: Option<SetupProposal<'static>>,
}

impl Lessee {
    /// Runs the setup draft's turn `run` to its report, inside the span
    /// naming this runner and the run.
    ///
    /// # Errors
    /// An identifier that is not canonical; nothing has started then. Every
    /// failure after that is reported; an unacknowledged report is returned.
    pub(crate) async fn setup(
        &self,
        run: &RunPayload<'_>,
        interrupt: CancellationToken,
    ) -> Result<()> {
        let identity = self.whoami.get(&self.plane).await;
        let span = run_span(identity, run);
        Box::pin(self.setup_assigned(run, interrupt).instrument(span)).await
    }

    async fn setup_assigned(
        &self,
        run: &RunPayload<'_>,
        interrupt: CancellationToken,
    ) -> Result<()> {
        let id = Uuid7::parse(&run.run_id)?;
        let _busy = self.holds.occupy(Occupant::Setup { run: id.clone() });
        let run_id = id.as_str();
        tracing::info!(run_id, event = EVENT_ACQUIRED);
        let meter = Arc::new(Meter::new(Arc::clone(&self.clock)));
        let started = Instant::now();
        let (sink, mut pump) = activity::channel(&self.plane, &id);
        let renewal = Renewal::new(
            &self.plane,
            &id,
            run.run_expires_at,
            self.clock.as_ref(),
            &meter,
        )
        .keep();
        let pumping = pump.run();
        let context = RunContext::new(&self.plane, &id, run.fencing_token);
        let work = self.work(run, &context, sink, &meter, &interrupt);
        tokio::pin!(work, renewal, pumping);
        let (mut renewing, mut draining) = (true, true);
        let mut cut = None;
        let (output, first_chunk) = loop {
            tokio::select! {
                biased;
                worked = &mut work => break worked,
                class = &mut renewal, if renewing => {
                    renewing = false;
                    interrupt.cancel();
                    cut.get_or_insert((class, DETAIL_RENEWAL));
                }
                () = self.halt.running().cancelled(), if !interrupt.is_cancelled() => {
                    interrupt.cancel();
                    cut.get_or_insert((FailureClass::RenewalTerminate, DETAIL_STOPPED));
                }
                () = &mut pumping, if draining => draining = false,
            }
        };
        let worked = self.ended(&id, output, first_chunk, cut);
        let reporting = self.report_setup(run, &id, &worked, &meter, started.elapsed());
        tokio::pin!(reporting);
        let reported = loop {
            tokio::select! {
                biased;
                reported = &mut reporting => break reported,
                _class = &mut renewal, if renewing => renewing = false,
                () = &mut pumping, if draining => draining = false,
            }
        };
        if draining
            && tokio::time::timeout(ACTIVITY_DRAIN_WAIT, &mut pumping)
                .await
                .is_err()
        {
            let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(error_code, run_id, event = EVENT_DRAIN_ABANDONED);
        }
        reported
    }

    /// The engine's setup loop. The sink is dropped once the loop ends, so
    /// the pump drains the run's last frames.
    async fn work(
        &self,
        run: &RunPayload<'_>,
        context: &RunContext<'_>,
        sink: ActivitySink,
        meter: &Meter,
        stop: &CancellationToken,
    ) -> (afr_agent::Result<SetupOutput>, Option<Duration>) {
        sink.run_started();
        let setup = SetupRun {
            run,
            context,
            events: &sink,
            meter,
            stop,
        };
        let output = self.agent.setup(setup).await;
        (output, sink.timing().first_chunk)
    }

    /// How the run ended: what the engine handed back, a Stop the daemon
    /// sent outranking a cut, and a cut outranking a clean end.
    fn ended(
        &self,
        id: &Uuid7,
        output: afr_agent::Result<SetupOutput>,
        first_chunk: Option<Duration>,
        cut: Option<(FailureClass, &'static str)>,
    ) -> Worked {
        let (ran, proposal) = match output {
            Ok(SetupOutput { result, proposal }) => {
                let output = RunOutput {
                    recovery: None,
                    task: None,
                    result,
                    memory: Vec::new(),
                    trace: None,
                    records: Vec::new(),
                    folded_through_sequence: None,
                };
                let ran = Ending::Ran {
                    output: Box::new(output),
                    first_chunk,
                };
                (ran, proposal)
            }
            Err(failure) => {
                let logged = failure.logged();
                let (error_code, reason, run_id) = (logged.error_code, logged.reason, id.as_str());
                tracing::warn!(error_code, reason, run_id, event = EVENT_ENGINE_FAILED);
                let failed = Ending::Failed {
                    class: FailureClass::StartupPosture,
                    detail: DETAIL_ENGINE,
                };
                (failed, None)
            }
        };
        let ending = if self.plane.stream.running.stopped(id) {
            ran.stopped()
        } else {
            match cut {
                Some((class, detail)) => ran.cut(class, detail),
                None => ran,
            }
        };
        Worked { ending, proposal }
    }

    /// The run's report, with the proposal it kept in its setup part, held
    /// until the daemon acknowledges it. The daemon reads a report with a
    /// setup part as a draft's.
    async fn report_setup(
        &self,
        run: &RunPayload<'_>,
        id: &Uuid7,
        worked: &Worked,
        meter: &Meter,
        wall: Duration,
    ) -> Result<()> {
        renew::flush(&self.plane, id, meter).await;
        let mut request = report(run, &worked.ending, meter, wall, None, None);
        request.setup = Some(SetupReport {
            proposal: worked.proposal.clone(),
        });
        let bytes = Zeroizing::new(serde_json::to_vec(&request).map_err(error::encode)?);
        until_acknowledged(&self.halt, self.plane.runtime, async || {
            self.plane.report(&bytes).await.map(Some)
        })
        .await?;
        let (run_id, outcome) = (id.as_str(), request.outcome.as_str());
        let kept = worked.proposal.is_some();
        tracing::info!(run_id, outcome, kept, event = EVENT_REPORTED);
        Ok(())
    }
}
