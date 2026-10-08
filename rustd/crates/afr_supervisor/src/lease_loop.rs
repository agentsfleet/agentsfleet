//! One lease, end to end.
//!
//! ```text
//!   admit ─► turn ─► bundle ─► hydrate ─► [take hold or prepare sandbox ─► land] ─► run
//!   ─► [hold frozen, or destroy] ─► memory push (fenced) ─► report spooled ─► report posted
//!   ─► activity drained
//!   └──────────── renewal alongside, until the report is answered ─────────┘
//! ```
//!
//! The engine admits the lease first: a policy naming a tool it cannot host is
//! refused before anything is prepared. A lease whose tools all run in the
//! supervisor starts no sandbox; any other runs in one, and one that cannot be
//! built ends the lease with a failed report. A sandbox that was built is held
//! for the fleet's next lease when the run ended processed (`crate::holds`) and
//! destroyed exactly once otherwise — the type consumes it — and an engine that
//! panics is caught so the teardown still runs. The report settles before the live tail
//! is waited on, and that wait is bounded: the tail is best-effort, the report
//! is the record.

use std::sync::Arc;
use std::time::Duration;

use afd_core::clock::Clock;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::spelling::to_spelling;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryHydrateResponse;
use afd_wire::report::FailureClass;
use afr_agent::{AgentEngine, Meter};
use afr_memory::Seed;
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
use crate::holds::Holds;
use crate::identity::{Whoami, lease_span};
use crate::memory;
use crate::renew::Renewal;
use crate::report::Ending;
use crate::report_spool::ReportSpool;
use crate::turns::FleetTurns;
use crate::workspace_clone::Mirrors;

mod checkout;
mod drive;
mod hold;
mod refusal;
mod settle;
mod workspace;

use self::hold::Worked;
use self::refusal::failed;

/// How long a settled lease waits for its live tail to finish posting.
pub(crate) const ACTIVITY_DRAIN_WAIT: Duration = Duration::from_secs(5);
const DETAIL_TURN: &str = "the worker pool was shutting down when the lease arrived";
const DETAIL_BUNDLE: &str = "the fleet bundle could not be fetched and verified";
const DETAIL_MEMORY: &str = "the fleet's memory could not be read";
const DETAIL_UNHOSTED: &str = "the fleet names a tool this runner cannot host";
const DETAIL_UNHOSTED_PROVIDER: &str =
    "the fleet names a model provider this runner does not speak";
const DETAIL_BLOCKED_ENDPOINT: &str =
    "the fleet names a model endpoint at a private or reserved address";
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
    /// Bound repositories' mirrors, fetched outside every sandbox.
    pub(crate) mirrors: Mirrors,
    /// What every sandbox enforces.
    pub(crate) limits: Limits,
    /// The wall clock the daemon's lease deadlines are written in.
    pub(crate) clock: Arc<dyn Clock>,
    /// Sandboxes held for their fleets' next leases.
    pub(crate) holds: Holds,
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
    /// What the run has spent so far, read into every renewal.
    meter: Meter,
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
            meter: Meter::default(),
        };
        let _busy = self.holds.occupy(run.ids.fleet.clone());
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
        // and reports the cut as the reason it ended.
        let Worked { ending: ran, kept } = worked;
        let mut ending = match cut {
            Some((class, detail)) => ran.cut(class, detail),
            None => ran,
        };
        {
            let settle = async {
                let held_until = self.keep(kept, &ending).await;
                let superseded = self.settle(&mut ending, started, held_until).await;
                if superseded && held_until.is_some() {
                    lessee.holds.supersede(self.ids.lease.clone());
                }
            };
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

    /// Everything that holds the fleet's turn: setup and the run. The sandbox
    /// the run used comes back with its ending, for the lease to hold or
    /// destroy once the cut and the report decide.
    ///
    /// The interrupt ends the run early, and the caller's ending replaces
    /// whatever this returns.
    async fn work(&self, turns: &FleetTurns, sink: ActivitySink) -> Worked {
        let lessee = self.lessee;
        let needs = match lessee.agent.admit(&self.lease.policy) {
            Ok(needs) => needs,
            Err(refusal) => return self.unhosted(&refusal).into(),
        };
        let claimed = tokio::select! {
            turn = turns.claim(&self.ids.fleet) => turn,
            () = self.interrupt.cancelled() => {
                return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL).into();
            }
        };
        let Some(_turn) = claimed else {
            return failed(FailureClass::StartupPosture, DETAIL_TURN).into();
        };
        let bundle = match &self.lease.bundle {
            Some(manifest) => match lessee
                .bundles
                .fetch(&lessee.plane, &manifest.content_hash)
                .await
            {
                Ok(bundle) => bundle,
                Err(failure) => {
                    return self
                        .refuse(&failure, EVENT_BUNDLE_FAILED, DETAIL_BUNDLE)
                        .into();
                }
            },
            None => None,
        };
        let hydrated = match memory::hydrate(&lessee.plane, &self.ids.fleet).await {
            Ok(hydrated) => hydrated,
            Err(failure) => {
                return self
                    .refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY)
                    .into();
            }
        };
        let memory = match hydrated.decode::<MemoryHydrateResponse<'_>>() {
            Ok(memory) => memory,
            Err(failure) => {
                return self
                    .refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY)
                    .into();
            }
        };
        let recaller = memory::Recaller::new(&lessee.plane, &self.ids.fleet, self.lease);
        let seed = Seed {
            window: &memory.memory,
            shared: &memory.shared,
            publish: memory.publish,
            recall: Some(&recaller),
        };
        if needs.sandbox {
            self.sandboxed(seed, bundle.as_ref(), sink).await
        } else {
            self.drive(seed, None, sink).await.into()
        }
    }
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
