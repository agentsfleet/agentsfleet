//! One run, end to end.
//!
//! ```text
//!   admit ─► turn ─► memory (the run's window, or read) ─► [egress, checkout plan] ─► run
//!                                  └─► [take hold or prepare ─► checkout] ─┘ beside it
//!   ─► [hold frozen, or kill] ─► current report retained in memory ─► report acknowledged
//!   ─► [release the killed sandbox's disk ∥ activity drained]
//!   └──────────── renewal alongside, until the report is answered ─────────┘
//! ```
//!
//! The engine admits the run first: a policy naming a tool it cannot host is
//! refused before anything is prepared. A run whose tools all run in the
//! supervisor starts no sandbox; any other builds one beside its turn, and a
//! build that fails leaves the turn going without it. A sandbox that was built is held
//! for the fleet's next run when the run ended processed (`crate::holds`) and
//! destroyed exactly once otherwise — the type consumes it — and an engine that
//! panics is caught so the teardown still runs. The report settles before the live tail
//! is waited on, and that wait is bounded: the tail is best-effort, the report
//! is the record.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::report::FailureClass;
use af_wire::run::RunPayload;
use afr_agent::Meter;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use crate::activity;
use crate::egress::Egress;
use crate::error::Result;
use crate::identity::run_span;
use crate::renew::Renewal;
use crate::report::Ending;
use crate::turns::FleetTurns;

mod build;
mod checkout;
mod drive;
mod hold;
mod lessee;
mod recovery;
mod recovery_boundary;
mod recovery_capture;
mod refusal;
mod segments;
mod settle;
mod task_workspace;
mod turn_recovery;
mod work;
mod workspace;

use self::hold::{Kept, Worked};
pub(crate) use self::lessee::Lessee;
use self::refusal::failed;
use self::segments::{Segment, Timeline};

/// How long a settled run waits for its live tail to finish posting.
pub(crate) const ACTIVITY_DRAIN_WAIT: Duration = Duration::from_secs(5);
const DETAIL_TURN: &str = "the worker pool was shutting down when the run arrived";
const DETAIL_MEMORY: &str = "the fleet's memory could not be read";
const DETAIL_UNHOSTED: &str = "the fleet names a tool this runner cannot host";
const DETAIL_UNHOSTED_PROVIDER: &str = "the selected model has no saved inference settings";
const DETAIL_CONFIGURATION: &str =
    "the fleet execution configuration cannot be admitted by this runner";
const DETAIL_RENEWAL: &str = "the daemon ended the run while it ran";
const DETAIL_STOPPED: &str = "this runner was told to stop while the run went on";
const EVENT_ACQUIRED: &str = "run_acquired";
const EVENT_COMPLETED: &str = "run_completed";
const EVENT_FAILED: &str = "run_failed";
const EVENT_STOPPED: &str = "run_stopped";
/// A fixed spelling, because dashboards and alerts match on it.
const EVENT_UNHOSTED: &str = "tool_refused_not_hosted";
const EVENT_UNHOSTED_PROVIDER: &str = "provider_refused_not_hosted";
const EVENT_LOAD_FAILED: &str = "memory_load_failed";
const EVENT_DRAIN_ABANDONED: &str = "activity_drain_abandoned";

/// One run's identifiers, parsed once.
pub(super) struct Ids {
    run: Uuid7,
    fleet: Uuid7,
}

/// One run while it runs: what it was given, its identifiers, and the token
/// that ends it early. Each step of the run is a method on it, so none
/// passes them along by hand.
pub(super) struct Run<'a> {
    lessee: &'a Lessee,
    payload: &'a RunPayload<'a>,
    /// The egress the runner was assigned when the run arrived.
    egress: &'a Egress,
    ids: Ids,
    interrupt: CancellationToken,
    decisions: crate::verbs::PendingDecisions,
    /// What the run has spent so far, read into every renewal, its lines
    /// stamped by the lessee's clock.
    meter: Arc<Meter>,
    /// When the run began, for its wall time.
    started: Instant,
    /// Where the run's time went, for its last line.
    timeline: Timeline,
}

/// Which of a run's side tasks are still going.
struct Live {
    renewing: bool,
    pumping: bool,
}

impl Lessee {
    /// Runs `run` to its report under `egress`, inside the span naming this
    /// runner and the run.
    ///
    /// # Errors
    /// Identifiers that are not canonical; nothing has started then. Every
    /// failure after that is reported; an unacknowledged terminal refusal is returned.
    pub(crate) async fn run(
        &self,
        turns: &FleetTurns,
        run: &RunPayload<'_>,
        egress: &Egress,
        interrupt: CancellationToken,
    ) -> Result<()> {
        let identity = self.whoami.get(&self.plane).await;
        let span = run_span(identity, run);
        Box::pin(
            self.run_assigned(turns, run, egress, interrupt)
                .instrument(span),
        )
        .await
    }

    async fn run_assigned(
        &self,
        turns: &FleetTurns,
        run: &RunPayload<'_>,
        egress: &Egress,
        interrupt: CancellationToken,
    ) -> Result<()> {
        let run = Run {
            lessee: self,
            payload: run,
            egress,
            ids: Ids {
                run: Uuid7::parse(&run.run_id)?,
                fleet: Uuid7::parse(&run.event.fleet_id)?,
            },
            interrupt,
            decisions: crate::verbs::PendingDecisions::default(),
            meter: Arc::new(Meter::new(Arc::clone(&self.clock))),
            started: Instant::now(),
            timeline: Timeline::default(),
        };
        let _busy = self.holds.occupy(crate::holds::Occupant::Fleet {
            run: run.ids.run.clone(),
            fleet: run.ids.fleet.clone(),
        });
        let run_id = run.ids.run.as_str();
        let event = EVENT_ACQUIRED;
        tracing::info!(run_id, event);
        Box::pin(run.run(turns)).await.map(drop)
    }
}

impl Run<'_> {
    /// The run, its renewal and its live tail, then the settle.
    async fn run(&self, turns: &FleetTurns) -> Result<Ending> {
        let lessee = self.lessee;
        let (sink, mut pump) = activity::channel(&lessee.plane, &self.ids.run);
        let renewal = Renewal::new(
            &lessee.plane,
            &self.ids.run,
            self.payload.run_expires_at,
            lessee.clock.as_ref(),
            &self.meter,
        );
        let pumping = pump.run();
        let renewal = renewal.keep();
        let work = self.work(turns, sink);
        tokio::pin!(pumping, renewal, work);
        let mut live = Live {
            renewing: true,
            pumping: true,
        };
        let mut cut = None;
        let worked = loop {
            tokio::select! {
                biased;
                worked = &mut work => break worked,
                class = &mut renewal, if live.renewing => {
                    live.renewing = false;
                    self.interrupt.cancel();
                    cut.get_or_insert((class, DETAIL_RENEWAL));
                }
                () = lessee.halt.running().cancelled(), if !self.interrupt.is_cancelled() => {
                    self.interrupt.cancel();
                    cut.get_or_insert((FailureClass::RenewalTerminate, DETAIL_STOPPED));
                }
                () = &mut pumping, if live.pumping => live.pumping = false,
            }
        };
        // A cut keeps what the run handed back, its tokens and memory with it,
        // and reports the cut as the reason it ended. A Stop the daemon sent,
        // as a Cancel or as a refused renewal, outranks the cut it caused.
        let Worked { ending: ran, kept } = worked;
        let ending = if lessee.plane.stream.running.stopped(&self.ids.run) {
            ran.stopped()
        } else {
            match cut {
                Some((class, detail)) => ran.cut(class, detail),
                None => ran,
            }
        };
        self.settle_and_drain(kept, ending, renewal, pumping, live)
            .await
    }

    /// The hold or the kill, and the report, with the renewal and the live
    /// tail still polled beside them; then the release of a killed sandbox's
    /// disk beside the wait the tail gets to drain. Timed as the report
    /// segment up to the release.
    async fn settle_and_drain(
        &self,
        kept: Option<Kept>,
        mut ending: Ending,
        mut renewal: Pin<&mut impl Future<Output = FailureClass>>,
        mut pumping: Pin<&mut impl Future<Output = ()>>,
        mut live: Live,
    ) -> Result<Ending> {
        let reporting = self.timeline.start(Segment::Report);
        let (fate, reported) = {
            let settle = async {
                let fate = self.keep(kept, &mut ending).await;
                let reported = self.settle(&mut ending, &fate, &reporting).await;
                if reported.is_err() && fate.held_until().is_some() {
                    self.lessee.holds.supersede(self.ids.run.clone());
                }
                (fate, reported)
            };
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    biased;
                    fate = &mut settle => break fate,
                    _class = &mut renewal, if live.renewing => live.renewing = false,
                    () = &mut pumping, if live.pumping => live.pumping = false,
                }
            }
        };
        reporting.stop();
        // The disk goes after the report, beside the tail's drain, so neither
        // waits on the other; the run returns only once both are done.
        let drained = async {
            !live.pumping
                || tokio::time::timeout(ACTIVITY_DRAIN_WAIT, &mut pumping)
                    .await
                    .is_ok()
        };
        let ((), drained) = tokio::join!(self.release(fate), drained);
        if !drained {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let run_id = self.ids.run.as_str();
            let event = EVENT_DRAIN_ABANDONED;
            tracing::warn!(
                error_code = code,
                run_id,
                event,
                "the live tail did not drain; the report already settled"
            );
        }
        self.conclude(&ending);
        reported?;
        Ok(ending)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/admission.rs"]
mod admit_tests;

#[cfg(test)]
#[path = "tests/records.rs"]
mod records_tests;
