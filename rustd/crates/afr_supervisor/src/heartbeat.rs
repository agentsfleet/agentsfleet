//! Liveness: the beat that carries what this host can do and brings back what
//! it is assigned.

use std::time::Duration;

use afd_core::limits::WorkerCount;
use afd_core::timing::HEARTBEAT_INTERVAL_MS;
use afd_wire::runner::{
    HeartbeatRequest, HeartbeatResponse, HeartbeatStatus, NetworkPolicy, SandboxTier,
};
use afr_sandbox::HostProbe;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::capability::{capability_report, selftest};
use crate::client::ControlPlane;
use crate::error::Result;

const EVENT_FAILED: &str = "heartbeat_failed";
const EVENT_UNAUTHORIZED: &str = "heartbeat_unauthorized";

/// What the daemon most recently told this runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assignment {
    /// Whether to take work, finish it, or stop.
    pub status: HeartbeatStatus,
    /// How many leases to run at once.
    pub workers: WorkerCount,
    /// How long until the next beat.
    pub interval: Duration,
}

impl Assignment {
    /// What a runner assumes before its first beat is answered.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            status: HeartbeatStatus::Ok,
            workers: WorkerCount::default(),
            interval: Duration::from_millis(HEARTBEAT_INTERVAL_MS.unsigned_abs()),
        }
    }

    /// Whether worker number `worker` should take new work.
    #[must_use]
    pub const fn takes_work(self, worker: u32) -> bool {
        matches!(self.status, HeartbeatStatus::Ok) && worker < self.workers.get()
    }
}

/// The beat, and what it remembers between beats.
#[derive(Debug)]
pub struct Heartbeat<'a> {
    plane: &'a ControlPlane,
    probe: &'a HostProbe,
    label: Option<(SandboxTier, NetworkPolicy)>,
    selftest_due: bool,
    last: Assignment,
}

impl<'a> Heartbeat<'a> {
    /// A beat over `plane`, reporting `probe`.
    #[must_use]
    pub fn new(plane: &'a ControlPlane, probe: &'a HostProbe) -> Self {
        Self {
            plane,
            probe,
            label: None,
            selftest_due: false,
            last: Assignment::initial(),
        }
    }

    /// Beats once, carrying a self-test when one was asked for.
    ///
    /// A self-test is labelled with the assignment it ran under, so it waits
    /// for the first beat that brings one.
    ///
    /// # Errors
    /// Any classified failure of the call, or a reply that does not decode.
    pub async fn beat(&mut self) -> Result<Assignment> {
        let selftest = self
            .label
            .filter(|_| self.selftest_due)
            .map(|(tier, network)| selftest(self.probe, tier, network));
        let request = HeartbeatRequest {
            capability_report: Some(capability_report(self.probe)),
            selftest,
        };
        let body = self.plane.heartbeat(&request).await?;
        let reply: HeartbeatResponse<'_> = body.decode()?;
        self.selftest_due = reply.selftest_requested;
        if let Some(policy) = &reply.assigned_policy {
            self.label = Some((policy.sandbox_tier, policy.network_policy));
            self.last.workers = WorkerCount::clamping(policy.worker_count);
        }
        self.last.status = reply.status;
        self.last.interval = Duration::from_millis(u64::from(reply.heartbeat_interval_ms));
        Ok(self.last)
    }

    /// Beats until `shutdown`, publishing each assignment.
    ///
    /// A refused token, or a `stop`, ends the runner: neither is answered by
    /// waiting. Any other failure keeps the last assignment and beats again.
    pub async fn keep_beating(
        mut self,
        assignment: &watch::Sender<Assignment>,
        shutdown: &CancellationToken,
    ) {
        loop {
            let interval = self.last.interval;
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(interval) => {}
            }
            match self.beat().await {
                Ok(beat) => {
                    if beat.status == HeartbeatStatus::Stop {
                        shutdown.cancel();
                    }
                    assignment.send_replace(beat);
                }
                Err(failure) => {
                    let code = failure.code().as_str();
                    if failure.is_unauthorized() {
                        let event = EVENT_UNAUTHORIZED;
                        tracing::error!(
                            error_code = code,
                            event,
                            "the daemon refused this runner's token"
                        );
                        shutdown.cancel();
                        return;
                    }
                    let event = EVENT_FAILED;
                    tracing::warn!(
                        error_code = code,
                        event,
                        "a heartbeat failed; the last assignment stands"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "heartbeat/tests.rs"]
mod tests;
