//! One lease, end to end.
//!
//! ```text
//!   turn ─► bundle ─► hydrate ─► prepare sandbox ─► run ──────────► destroy
//!                                                    │  renewal alongside;
//!                                                    │  a 4xx ends the run
//!   ─► memory push (fenced) ─► report spooled ─► report posted
//! ```
//!
//! Nothing runs without a sandbox: one that cannot be built ends the lease with
//! a failed report. A sandbox that was built is destroyed exactly once, on
//! every path out of the run — the type consumes it.

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::{MemoryDelta, MemoryHydrateResponse};
use afd_wire::report::FailureClass;
use afr_agent::{AgentEngine, AgentRun};
use afr_sandbox::{Engine, Limits, Sandbox, SandboxRequest};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::activity;
use crate::bundles::BundleCache;
use crate::client::{ControlPlane, retrying};
use crate::error::Result;
use crate::memory;
use crate::renew::Renewal;
use crate::report::{Ending, report};
use crate::report_spool::{Delivery, ReportSpool};
use crate::turns::FleetTurns;

const DETAIL_BUNDLE: &str = "the fleet bundle could not be fetched and verified";
const DETAIL_MEMORY: &str = "the fleet's memory could not be read";
const DETAIL_SANDBOX: &str = "this host could not build a sandbox for the run";
const DETAIL_ENGINE: &str = "the agent engine stopped before the turn ended";
const DETAIL_RENEWAL: &str = "the daemon ended the lease while it ran";
const EVENT_ACQUIRED: &str = "lease_acquired";
const EVENT_COMPLETED: &str = "lease_completed";
const EVENT_FAILED: &str = "lease_failed";
const EVENT_SANDBOX_REFUSED: &str = "sandbox_refused";
const EVENT_BUNDLE_FAILED: &str = "bundle_download_failed";
const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";
const EVENT_HYDRATE_FAILED: &str = "memory_hydrate_failed";
const EVENT_CAPTURE_FAILED: &str = "memory_capture_post_failed";
const EVENT_ENGINE_FAILED: &str = "engine_run_failed";
const EVENT_SPOOL_KEPT: &str = "report_spool_kept";

/// Everything a lease needs, shared by every worker.
#[derive(Debug)]
pub struct Lessee {
    /// The daemon.
    pub plane: ControlPlane,
    /// Builds each lease's sandbox.
    pub engine: Box<dyn Engine>,
    /// Runs each lease's turn.
    pub agent: Box<dyn AgentEngine>,
    /// Where reports wait to be posted.
    pub spool: ReportSpool,
    /// Verified fleet bundles.
    pub bundles: BundleCache,
    /// What every sandbox enforces.
    pub limits: Limits,
}

/// One lease's identifiers, parsed once.
struct Ids {
    lease: Uuid7,
    fleet: Uuid7,
}

impl Lessee {
    /// Runs `lease` to its report.
    ///
    /// # Errors
    /// Identifiers that are not canonical, or a report that cannot be spooled.
    /// A report the daemon cannot take yet stays spooled and is not an error.
    pub async fn run(
        &self,
        turns: &FleetTurns,
        lease: &LeasePayload<'_>,
    ) -> Result<Option<Delivery>> {
        let ids = Ids {
            lease: Uuid7::parse(&lease.lease_id)?,
            fleet: Uuid7::parse(&lease.event.fleet_id)?,
        };
        let lease_id = ids.lease.as_str();
        let event = EVENT_ACQUIRED;
        tracing::info!(lease_id, event);
        let started = Instant::now();
        let ended = CancellationToken::new();
        let work = self.work(turns, lease, &ids, &ended);
        tokio::pin!(work);
        let mut ending = tokio::select! {
            ending = &mut work => ending,
            class = Renewal::new(&self.plane, &ids.lease).keep() => {
                ended.cancel();
                drop((&mut work).await);
                failed(class, DETAIL_RENEWAL)
            }
        };
        if let Ending::Ran { output, .. } = &mut ending {
            self.capture(&ids, lease, std::mem::take(&mut output.memory))
                .await;
        }
        let delivery = self.settle(&ids, lease, &ending, started.elapsed()).await?;
        let event = match ending {
            Ending::Ran { .. } => EVENT_COMPLETED,
            Ending::Failed { .. } => EVENT_FAILED,
        };
        tracing::info!(lease_id, event);
        Ok(delivery)
    }

    /// Everything that holds the fleet's turn: setup, the run, teardown.
    ///
    /// `ended` is the renewal's refusal; the run stops on it and the sandbox is
    /// still destroyed. A refusal that arrives before the run starts makes the
    /// rest moot, and the caller's ending wins.
    async fn work(
        &self,
        turns: &FleetTurns,
        lease: &LeasePayload<'_>,
        ids: &Ids,
        ended: &CancellationToken,
    ) -> Ending {
        let _turn = tokio::select! {
            turn = turns.claim(&ids.fleet) => turn,
            () = ended.cancelled() => return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL),
        };
        if let Some(bundle) = &lease.bundle
            && let Err(failure) = self.bundles.fetch(&self.plane, &bundle.content_hash).await
        {
            return refuse(&failure, EVENT_BUNDLE_FAILED, DETAIL_BUNDLE);
        }
        let unreadable =
            |failure: &crate::Error| note(failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY);
        let Ok(hydrated) = memory::hydrate(&self.plane, &ids.fleet)
            .await
            .inspect_err(unreadable)
        else {
            return failed(FailureClass::StartupPosture, DETAIL_MEMORY);
        };
        let Ok(memory) = hydrated
            .decode::<MemoryHydrateResponse<'_>>()
            .inspect_err(unreadable)
        else {
            return failed(FailureClass::StartupPosture, DETAIL_MEMORY);
        };
        let request = SandboxRequest {
            lease_id: ids.lease.as_str(),
            limits: self.limits,
        };
        let sandbox = match self.engine.prepare(request).await {
            Ok(sandbox) => sandbox,
            Err(failure) => {
                let code = crate::Error::from(failure).code().as_str();
                let lease_id = ids.lease.as_str();
                let event = EVENT_SANDBOX_REFUSED;
                let detail = DETAIL_SANDBOX;
                tracing::error!(error_code = code, lease_id, event, detail);
                return failed(FailureClass::StartupPosture, DETAIL_SANDBOX);
            }
        };
        let ending = self
            .drive(lease, ids, &memory.memory, sandbox.as_ref(), ended)
            .await;
        if let Err(failure) = sandbox.destroy().await {
            let code = crate::Error::from(failure).code().as_str();
            let event = EVENT_DESTROY_FAILED;
            tracing::warn!(
                error_code = code,
                event,
                "a sandbox did not tear down cleanly"
            );
        }
        ending
    }

    /// The turn itself, with its activity pumped alongside.
    async fn drive(
        &self,
        lease: &LeasePayload<'_>,
        ids: &Ids,
        memory: &[MemoryDelta<'_>],
        sandbox: &dyn Sandbox,
        ended: &CancellationToken,
    ) -> Ending {
        let (sink, mut pump) = activity::channel(&self.plane, &ids.lease);
        let run = async move {
            let run = self.agent.run(AgentRun {
                lease,
                memory,
                executor: Some(sandbox.executor()),
                events: &sink,
            });
            tokio::select! {
                output = run => Some(output),
                () = ended.cancelled() => None,
            }
        };
        let (output, ()) = tokio::join!(run, pump.run());
        match output {
            Some(Ok(output)) => Ending::Ran {
                output,
                first_chunk: pump.pumped().first_chunk,
            },
            Some(Err(failure)) => {
                let code = failure.code().as_str();
                let event = EVENT_ENGINE_FAILED;
                tracing::warn!(error_code = code, event, "the agent engine failed");
                failed(FailureClass::RunnerCrash, DETAIL_ENGINE)
            }
            None => failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL),
        }
    }

    /// Pushes the run's memory before the report settles the lease.
    async fn capture(
        &self,
        ids: &Ids,
        lease: &LeasePayload<'_>,
        memory: Vec<MemoryDelta<'static>>,
    ) {
        if let Err(failure) = memory::capture(&self.plane, &ids.fleet, lease, memory).await {
            let code = failure.code().as_str();
            let event = EVENT_CAPTURE_FAILED;
            tracing::warn!(
                error_code = code,
                event,
                "the run's memory was not written back"
            );
        }
    }

    /// Spools the report, then posts it until the daemon answers.
    async fn settle(
        &self,
        ids: &Ids,
        lease: &LeasePayload<'_>,
        ending: &Ending,
        wall: Duration,
    ) -> Result<Option<Delivery>> {
        let spooled = self.spool.hold(&ids.lease, &report(lease, ending, wall))?;
        match retrying(|| spooled.clone().deliver(&self.plane)).await {
            Ok(delivery) => Ok(Some(delivery)),
            Err(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_SPOOL_KEPT;
                tracing::warn!(
                    error_code = code,
                    event,
                    "the report stays spooled for the next boot"
                );
                Ok(None)
            }
        }
    }
}

/// An ending that never ran the turn.
const fn failed(class: FailureClass, detail: &'static str) -> Ending {
    Ending::Failed { class, detail }
}

/// Logs why a lease could not start, and ends it at startup.
fn refuse(failure: &crate::Error, event: &'static str, detail: &'static str) -> Ending {
    note(failure, event, detail);
    failed(FailureClass::StartupPosture, detail)
}

/// Logs why a lease could not start.
fn note(failure: &crate::Error, event: &'static str, detail: &'static str) {
    let code = failure.code().as_str();
    tracing::warn!(error_code = code, event, detail);
}

#[cfg(test)]
#[path = "lease_loop/tests.rs"]
mod tests;
