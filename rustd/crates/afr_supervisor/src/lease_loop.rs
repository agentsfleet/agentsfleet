//! One lease, end to end.
//!
//! ```text
//!   turn ─► bundle ─► hydrate ─► prepare sandbox ─► run ─► destroy
//!   ─► memory push (fenced) ─► report spooled ─► report posted ─► activity drained
//!   └──────────── renewal alongside, until the report is answered ─────────┘
//! ```
//!
//! Nothing runs without a sandbox: one that cannot be built ends the lease with
//! a failed report. A sandbox that was built is destroyed exactly once — the
//! type consumes it — and an engine that panics is caught so the teardown still
//! runs. The report settles before the live tail is waited on, and that wait is
//! bounded: the tail is best-effort, the report is the record.

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use afd_core::clock::Clock;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::{MemoryDelta, MemoryHydrateResponse};
use afd_wire::report::FailureClass;
use afr_agent::{AgentEngine, AgentRun};
use afr_sandbox::{Engine, Limits, Sandbox, SandboxRequest};
use futures_util::FutureExt as _;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::activity::{self, ActivitySink};
use crate::bundles::BundleCache;
use crate::client::ControlPlane;
use crate::error::Result;
use crate::halt::Halt;
use crate::memory;
use crate::renew::Renewal;
use crate::report::Ending;
use crate::report_spool::ReportSpool;
use crate::turns::FleetTurns;

mod settle;

/// How long a settled lease waits for its live tail to finish posting.
pub(crate) const ACTIVITY_DRAIN_WAIT: Duration = Duration::from_secs(5);
const DETAIL_TURN: &str = "the worker pool was shutting down when the lease arrived";
const DETAIL_BUNDLE: &str = "the fleet bundle could not be fetched and verified";
const DETAIL_MEMORY: &str = "the fleet's memory could not be read";
const DETAIL_SANDBOX: &str = "this host could not build a sandbox for the run";
const DETAIL_ENGINE: &str = "the agent engine stopped before the turn ended";
const DETAIL_PANIC: &str = "the agent engine panicked";
const DETAIL_RENEWAL: &str = "the daemon ended the lease while it ran";
const DETAIL_STOPPED: &str = "this runner was told to stop while the run went on";
const EVENT_ACQUIRED: &str = "lease_acquired";
const EVENT_COMPLETED: &str = "lease_completed";
const EVENT_FAILED: &str = "lease_failed";
const EVENT_SANDBOX_REFUSED: &str = "sandbox_refused";
const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";
const EVENT_BUNDLE_FAILED: &str = "bundle_download_failed";
const EVENT_HYDRATE_FAILED: &str = "memory_hydrate_failed";
const EVENT_ENGINE_FAILED: &str = "engine_run_failed";
const EVENT_ENGINE_PANICKED: &str = "engine_panicked";
const EVENT_DRAIN_ABANDONED: &str = "activity_drain_abandoned";

/// Everything a lease needs, shared by every worker.
#[derive(Debug)]
pub(crate) struct Lessee {
    /// The daemon.
    pub(crate) plane: ControlPlane,
    /// Builds each lease's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// Runs each lease's turn.
    pub(crate) agent: Box<dyn AgentEngine>,
    /// Where reports wait to be posted.
    pub(crate) spool: ReportSpool,
    /// Verified fleet bundles.
    pub(crate) bundles: BundleCache,
    /// What every sandbox enforces.
    pub(crate) limits: Limits,
    /// The wall clock the daemon's lease deadlines are written in.
    pub(crate) clock: Box<dyn Clock>,
    /// How the runner stops.
    pub(crate) halt: Halt,
    /// Rung when a report stays spooled, so the drain takes it over.
    pub(crate) held: Notify,
}

/// One lease's identifiers, parsed once.
pub(super) struct Ids {
    lease: Uuid7,
    fleet: Uuid7,
}

/// Which of a lease's side tasks are still going.
struct Live {
    renewing: bool,
    pumping: bool,
}

impl Lessee {
    /// Runs `lease` to its report.
    ///
    /// # Errors
    /// Identifiers that are not canonical; nothing has started then. Every
    /// failure after that ends in a report, spooled or posted.
    pub(crate) async fn run(&self, turns: &FleetTurns, lease: &LeasePayload<'_>) -> Result<()> {
        let ids = Ids {
            lease: Uuid7::parse(&lease.lease_id)?,
            fleet: Uuid7::parse(&lease.event.fleet_id)?,
        };
        let lease_id = ids.lease.as_str();
        let event = EVENT_ACQUIRED;
        tracing::info!(lease_id, event);
        match self.lease(turns, lease, &ids).await {
            Ending::Ran { .. } => {
                let event = EVENT_COMPLETED;
                tracing::info!(lease_id, event);
            }
            Ending::Failed { class, detail } => {
                let class = format!("{class:?}");
                let event = EVENT_FAILED;
                tracing::info!(lease_id, class, detail, event);
            }
        }
        Ok(())
    }

    /// The run, its renewal and its live tail, then the settle.
    async fn lease(&self, turns: &FleetTurns, lease: &LeasePayload<'_>, ids: &Ids) -> Ending {
        let started = Instant::now();
        let (sink, mut pump) = activity::channel(&self.plane, &ids.lease);
        let interrupt = CancellationToken::new();
        let renewal = Renewal::new(
            &self.plane,
            &ids.lease,
            lease.lease_expires_at,
            self.clock.as_ref(),
        );
        let pumping = pump.run();
        let renewal = renewal.keep();
        let work = self.work(turns, lease, ids, sink, &interrupt);
        tokio::pin!(pumping, renewal, work);
        let mut live = Live {
            renewing: true,
            pumping: true,
        };
        let mut cut = None;
        let ran = loop {
            tokio::select! {
                biased;
                ending = &mut work => break ending,
                class = &mut renewal, if live.renewing => {
                    live.renewing = false;
                    interrupt.cancel();
                    cut.get_or_insert(failed(class, DETAIL_RENEWAL));
                }
                () = self.halt.running().cancelled(), if !interrupt.is_cancelled() => {
                    interrupt.cancel();
                    cut.get_or_insert(failed(FailureClass::RenewalTerminate, DETAIL_STOPPED));
                }
                () = &mut pumping, if live.pumping => live.pumping = false,
            }
        };
        let mut ending = cut.unwrap_or(ran);
        {
            let settle = self.settle(ids, lease, &mut ending, started);
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    biased;
                    () = &mut settle => break,
                    _class = &mut renewal, if live.renewing => live.renewing = false,
                    () = &mut pumping, if live.pumping => live.pumping = false,
                }
            }
        }
        if live.pumping
            && tokio::time::timeout(ACTIVITY_DRAIN_WAIT, &mut pumping)
                .await
                .is_err()
        {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let lease_id = ids.lease.as_str();
            let event = EVENT_DRAIN_ABANDONED;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "the live tail did not drain; the report already settled"
            );
        }
        ending
    }

    /// Everything that holds the fleet's turn: setup, the run, teardown.
    ///
    /// `interrupt` ends the run early; the sandbox is still destroyed, and the
    /// caller's ending replaces whatever this returns.
    async fn work(
        &self,
        turns: &FleetTurns,
        lease: &LeasePayload<'_>,
        ids: &Ids,
        sink: ActivitySink,
        interrupt: &CancellationToken,
    ) -> Ending {
        let claimed = tokio::select! {
            turn = turns.claim(&ids.fleet) => turn,
            () = interrupt.cancelled() => return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL),
        };
        let Some(_turn) = claimed else {
            return failed(FailureClass::StartupPosture, DETAIL_TURN);
        };
        if let Some(bundle) = &lease.bundle
            && let Err(failure) = self.bundles.fetch(&self.plane, &bundle.content_hash).await
        {
            return refuse(ids, &failure, EVENT_BUNDLE_FAILED, DETAIL_BUNDLE);
        }
        let hydrated = match memory::hydrate(&self.plane, &ids.fleet).await {
            Ok(hydrated) => hydrated,
            Err(failure) => return refuse(ids, &failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY),
        };
        let memory = match hydrated.decode::<MemoryHydrateResponse<'_>>() {
            Ok(memory) => memory,
            Err(failure) => return refuse(ids, &failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY),
        };
        let request = SandboxRequest {
            lease_id: ids.lease.as_str(),
            limits: self.limits,
        };
        let sandbox = match self.engine.prepare(request).await {
            Ok(sandbox) => sandbox,
            Err(failure) => {
                let code = failure.code().as_str();
                let lease_id = ids.lease.as_str();
                let event = EVENT_SANDBOX_REFUSED;
                let detail = DETAIL_SANDBOX;
                tracing::error!(error_code = code, lease_id, event, detail);
                return failed(FailureClass::StartupPosture, DETAIL_SANDBOX);
            }
        };
        let ending = self
            .drive(
                lease,
                ids,
                &memory.memory,
                sandbox.as_ref(),
                sink,
                interrupt,
            )
            .await;
        if let Err(failure) = sandbox.destroy().await {
            let code = failure.code().as_str();
            let lease_id = ids.lease.as_str();
            let event = EVENT_DESTROY_FAILED;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "a sandbox did not tear down cleanly"
            );
        }
        ending
    }

    /// The turn itself. A panicking engine is caught here, inside the
    /// sandbox's lifetime, so the caller still destroys it.
    async fn drive(
        &self,
        lease: &LeasePayload<'_>,
        ids: &Ids,
        memory: &[MemoryDelta<'_>],
        sandbox: &dyn Sandbox,
        sink: ActivitySink,
        interrupt: &CancellationToken,
    ) -> Ending {
        let run = AssertUnwindSafe(self.agent.run(AgentRun {
            lease,
            memory,
            executor: Some(sandbox.executor()),
            events: &sink,
        }))
        .catch_unwind();
        let output = tokio::select! {
            output = run => Some(output),
            () = interrupt.cancelled() => None,
        };
        let first_chunk = sink.first_chunk();
        drop(sink);
        let lease_id = ids.lease.as_str();
        match output {
            Some(Ok(Ok(output))) => Ending::Ran {
                output,
                first_chunk,
            },
            Some(Ok(Err(failure))) => {
                let code = failure.code().as_str();
                let event = EVENT_ENGINE_FAILED;
                tracing::warn!(
                    error_code = code,
                    lease_id,
                    event,
                    "the agent engine failed"
                );
                failed(FailureClass::RunnerCrash, DETAIL_ENGINE)
            }
            Some(Err(_panic)) => {
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let event = EVENT_ENGINE_PANICKED;
                tracing::error!(
                    error_code = code,
                    lease_id,
                    event,
                    "the agent engine panicked; its sandbox is still torn down"
                );
                failed(FailureClass::RunnerCrash, DETAIL_PANIC)
            }
            None => failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL),
        }
    }
}

/// An ending that never ran the turn to its end.
const fn failed(class: FailureClass, detail: &'static str) -> Ending {
    Ending::Failed { class, detail }
}

/// Logs why a lease could not start, and ends it at startup.
fn refuse(ids: &Ids, failure: &crate::Error, event: &'static str, detail: &'static str) -> Ending {
    let code = failure.code().as_str();
    let lease_id = ids.lease.as_str();
    tracing::warn!(error_code = code, lease_id, event, detail);
    failed(FailureClass::StartupPosture, detail)
}

#[cfg(test)]
#[path = "lease_loop/tests.rs"]
mod tests;
