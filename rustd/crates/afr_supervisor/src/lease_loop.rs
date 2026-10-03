//! One lease, end to end.
//!
//! ```text
//!   admit ─► turn ─► bundle ─► hydrate ─► [prepare sandbox ─► land] ─► run ─► [destroy]
//!   ─► memory push (fenced) ─► report spooled ─► report posted ─► activity drained
//!   └──────────── renewal alongside, until the report is answered ─────────┘
//! ```
//!
//! The engine admits the lease first: a policy naming a tool it cannot host is
//! refused before anything is prepared. A lease whose tools all run in the
//! supervisor starts no sandbox; any other runs in one, and one that cannot be
//! built ends the lease with a failed report. A sandbox that was built is
//! destroyed exactly once — the type consumes it — and an engine that panics is
//! caught so the teardown still runs. The report settles before the live tail
//! is waited on, and that wait is bounded: the tail is best-effort, the report
//! is the record.

use std::time::Duration;

use afd_core::clock::Clock;
use afd_core::error_code::{self, Coded};
use afd_core::id::Uuid7;
use afd_core::spelling::to_spelling;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryHydrateResponse;
use afd_wire::report::FailureClass;
use afr_agent::{AgentEngine, Unhosted};
use afr_sandbox::{Engine, Limits};
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use crate::activity::{self, ActivitySink};
use crate::bundles::BundleCache;
use crate::client::ControlPlane;
use crate::error::Result;
use crate::halt::Halt;
use crate::identity::{Whoami, lease_span};
use crate::memory;
use crate::renew::Renewal;
use crate::report::Ending;
use crate::report_spool::ReportSpool;
use crate::turns::FleetTurns;

mod drive;
mod settle;
mod workspace;

/// How long a settled lease waits for its live tail to finish posting.
pub(crate) const ACTIVITY_DRAIN_WAIT: Duration = Duration::from_secs(5);
const DETAIL_TURN: &str = "the worker pool was shutting down when the lease arrived";
const DETAIL_BUNDLE: &str = "the fleet bundle could not be fetched and verified";
const DETAIL_MEMORY: &str = "the fleet's memory could not be read";
const DETAIL_UNHOSTED: &str = "the fleet names a tool this runner cannot host";
const DETAIL_UNHOSTED_PROVIDER: &str =
    "the fleet names a model provider this runner does not speak";
const DETAIL_RENEWAL: &str = "the daemon ended the lease while it ran";
const DETAIL_STOPPED: &str = "this runner was told to stop while the run went on";
const EVENT_ACQUIRED: &str = "lease_acquired";
const EVENT_COMPLETED: &str = "lease_completed";
const EVENT_FAILED: &str = "lease_failed";
/// The Zig tool bridge's spelling, kept so its dashboards still match.
const EVENT_UNHOSTED: &str = "tool_refused_not_hosted";
const EVENT_UNHOSTED_PROVIDER: &str = "provider_refused_not_hosted";
const EVENT_BUNDLE_FAILED: &str = "bundle_download_failed";
const EVENT_HYDRATE_FAILED: &str = "memory_hydrate_failed";
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
    /// Which runner this is, for every lease's span.
    pub(crate) whoami: Whoami,
}

/// One lease's identifiers, parsed once.
pub(super) struct Ids {
    lease: Uuid7,
    fleet: Uuid7,
}

/// One lease while it runs: what it was given, its identifiers, and the token
/// that ends it early. Each step of the lease is a method on it, so none
/// passes them along by hand.
pub(super) struct LeaseRun<'a> {
    lessee: &'a Lessee,
    lease: &'a LeasePayload<'a>,
    ids: Ids,
    interrupt: CancellationToken,
}

/// Which of a lease's side tasks are still going.
struct Live {
    renewing: bool,
    pumping: bool,
}

impl Lessee {
    /// Runs `lease` to its report, inside the span naming this runner and
    /// the lease.
    ///
    /// # Errors
    /// Identifiers that are not canonical; nothing has started then. Every
    /// failure after that ends in a report, spooled or posted.
    pub(crate) async fn run(&self, turns: &FleetTurns, lease: &LeasePayload<'_>) -> Result<()> {
        let identity = self.whoami.get(&self.plane).await;
        let span = lease_span(identity, lease);
        self.run_lease(turns, lease).instrument(span).await
    }

    async fn run_lease(&self, turns: &FleetTurns, lease: &LeasePayload<'_>) -> Result<()> {
        let run = LeaseRun {
            lessee: self,
            lease,
            ids: Ids {
                lease: Uuid7::parse(&lease.lease_id)?,
                fleet: Uuid7::parse(&lease.event.fleet_id)?,
            },
            interrupt: CancellationToken::new(),
        };
        let lease_id = run.ids.lease.as_str();
        let event = EVENT_ACQUIRED;
        tracing::info!(lease_id, event);
        match run.lease(turns).await {
            Ending::Ran { .. } => {
                let event = EVENT_COMPLETED;
                tracing::info!(lease_id, event);
            }
            Ending::Failed { class, detail } => {
                // The spelling the report carries, so the log and the report
                // name a failure alike.
                let class = to_spelling(&class);
                let class = class.as_deref();
                let event = EVENT_FAILED;
                tracing::info!(lease_id, class, detail, event);
            }
        }
        Ok(())
    }
}

impl LeaseRun<'_> {
    /// The run, its renewal and its live tail, then the settle.
    async fn lease(&self, turns: &FleetTurns) -> Ending {
        let lessee = self.lessee;
        let started = Instant::now();
        let (sink, mut pump) = activity::channel(&lessee.plane, &self.ids.lease);
        let renewal = Renewal::new(
            &lessee.plane,
            &self.ids.lease,
            self.lease.lease_expires_at,
            lessee.clock.as_ref(),
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
        let ran = loop {
            tokio::select! {
                biased;
                ending = &mut work => break ending,
                class = &mut renewal, if live.renewing => {
                    live.renewing = false;
                    self.interrupt.cancel();
                    cut.get_or_insert(failed(class, DETAIL_RENEWAL));
                }
                () = lessee.halt.running().cancelled(), if !self.interrupt.is_cancelled() => {
                    self.interrupt.cancel();
                    cut.get_or_insert(failed(FailureClass::RenewalTerminate, DETAIL_STOPPED));
                }
                () = &mut pumping, if live.pumping => live.pumping = false,
            }
        };
        let mut ending = cut.unwrap_or(ran);
        {
            let settle = self.settle(&mut ending, started);
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
            let lease_id = self.ids.lease.as_str();
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
    /// The interrupt ends the run early; a sandbox is still destroyed, and
    /// the caller's ending replaces whatever this returns.
    async fn work(&self, turns: &FleetTurns, sink: ActivitySink) -> Ending {
        let lessee = self.lessee;
        let needs = match lessee.agent.admit(&self.lease.policy) {
            Ok(needs) => needs,
            Err(refusal) => return self.unhosted(&refusal),
        };
        let claimed = tokio::select! {
            turn = turns.claim(&self.ids.fleet) => turn,
            () = self.interrupt.cancelled() => {
                return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL);
            }
        };
        let Some(_turn) = claimed else {
            return failed(FailureClass::StartupPosture, DETAIL_TURN);
        };
        let bundle = match &self.lease.bundle {
            Some(manifest) => match lessee
                .bundles
                .fetch(&lessee.plane, &manifest.content_hash)
                .await
            {
                Ok(bundle) => bundle,
                Err(failure) => return self.refuse(&failure, EVENT_BUNDLE_FAILED, DETAIL_BUNDLE),
            },
            None => None,
        };
        let hydrated = match memory::hydrate(&lessee.plane, &self.ids.fleet).await {
            Ok(hydrated) => hydrated,
            Err(failure) => return self.refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY),
        };
        let memory = match hydrated.decode::<MemoryHydrateResponse<'_>>() {
            Ok(memory) => memory,
            Err(failure) => return self.refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY),
        };
        if needs.sandbox {
            self.sandboxed(&memory.memory, bundle.as_ref(), sink).await
        } else {
            self.drive(&memory.memory, None, sink).await
        }
    }

    /// Logs a lease whose policy names a tool or a model provider the
    /// engine cannot host, and ends it before anything was prepared for it.
    fn unhosted(&self, refusal: &afr_agent::Error) -> Ending {
        let (event, detail, name) = match refusal.unhosted() {
            Some(Unhosted::Provider(name)) => (
                EVENT_UNHOSTED_PROVIDER,
                DETAIL_UNHOSTED_PROVIDER,
                Some(name),
            ),
            Some(Unhosted::Tool(name)) => (EVENT_UNHOSTED, DETAIL_UNHOSTED, Some(name)),
            None => (EVENT_UNHOSTED, DETAIL_UNHOSTED, None),
        };
        let code = refusal.code().as_str();
        let lease_id = self.ids.lease.as_str();
        tracing::error!(error_code = code, lease_id, name, event);
        failed(FailureClass::StartupPosture, detail)
    }

    /// Logs why a lease could not start, and ends it at startup.
    fn refuse(&self, failure: &impl Coded, event: &'static str, detail: &'static str) -> Ending {
        self.fail(failure, FailureClass::StartupPosture, event, detail)
    }

    /// Logs a failure from any crate the lease runs through, and ends the
    /// lease as `class`.
    fn fail(
        &self,
        failure: &impl Coded,
        class: FailureClass,
        event: &'static str,
        detail: &'static str,
    ) -> Ending {
        let code = failure.code().as_str();
        let lease_id = self.ids.lease.as_str();
        tracing::warn!(error_code = code, lease_id, event, detail);
        failed(class, detail)
    }
}

/// An ending that never ran the turn to its end.
const fn failed(class: FailureClass, detail: &'static str) -> Ending {
    Ending::Failed { class, detail }
}

#[cfg(test)]
#[path = "lease_loop/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lease_loop/admit_tests.rs"]
mod admit_tests;

#[cfg(test)]
#[path = "lease_loop/records_tests.rs"]
mod records_tests;
