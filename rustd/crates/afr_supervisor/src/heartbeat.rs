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

use crate::capability::{capability_report, selftest};
use crate::client::{ControlPlane, endless};
use crate::error::Result;
use crate::halt::Halt;

/// The shortest pause between beats, whatever the daemon asks: a reply saying
/// zero must not turn the heartbeat into a busy loop.
pub(crate) const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_FAILED: &str = "heartbeat_failed";

/// What the daemon most recently told this runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Assignment {
    /// Whether to take work, finish it, or stop.
    pub(crate) status: HeartbeatStatus,
    /// How many leases to run at once; zero until a policy arrives.
    pub(crate) workers: u32,
    /// How long until the next beat.
    pub(crate) interval: Duration,
}

impl Assignment {
    /// What a runner assumes before its first beat is answered: no work, since
    /// it has no policy to work under.
    pub(crate) const fn initial() -> Self {
        Self {
            status: HeartbeatStatus::Ok,
            workers: 0,
            interval: Duration::from_millis(HEARTBEAT_INTERVAL_MS.unsigned_abs()),
        }
    }

    /// Whether worker number `worker` should take new work.
    pub(crate) const fn takes_work(self, worker: u32) -> bool {
        matches!(self.status, HeartbeatStatus::Ok) && worker < self.workers
    }
}

/// The beat, and what it remembers between beats.
#[derive(Debug)]
pub(crate) struct Heartbeat<'a> {
    plane: &'a ControlPlane,
    probe: &'a HostProbe,
    label: Option<(SandboxTier, NetworkPolicy)>,
    selftest_due: bool,
    last: Assignment,
}

impl<'a> Heartbeat<'a> {
    /// A beat over `plane`, reporting `probe`.
    pub(crate) const fn new(plane: &'a ControlPlane, probe: &'a HostProbe) -> Self {
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
    /// A null policy is a row the daemon could not read, and the runner fails
    /// closed on it: no workers, and no label for a self-test to run under.
    pub(crate) async fn beat(&mut self) -> Result<Assignment> {
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
        let policy = reply.assigned_policy.as_ref();
        self.selftest_due = reply.selftest_requested;
        self.label = policy.map(|policy| (policy.sandbox_tier, policy.network_policy));
        self.last = Assignment {
            status: reply.status,
            workers: policy.map_or(0, |policy| WorkerCount::clamping(policy.worker_count).get()),
            interval: Duration::from_millis(u64::from(reply.heartbeat_interval_ms))
                .max(MIN_HEARTBEAT_INTERVAL),
        };
        Ok(self.last)
    }

    /// Beats until the runner stops serving, publishing each assignment.
    ///
    /// A `stop` ends the runner, leases in flight included, and so does a
    /// refused token. Any other failure keeps the last assignment and beats
    /// again, sooner the first time and backing off after.
    pub(crate) async fn keep_beating(
        mut self,
        assignment: &watch::Sender<Assignment>,
        halt: &Halt,
    ) {
        let mut retries = endless();
        let mut pause = Duration::ZERO;
        loop {
            tokio::select! {
                () = halt.serving().cancelled() => return,
                () = tokio::time::sleep(pause) => {}
            }
            match self.beat().await {
                Ok(beat) => {
                    retries = endless();
                    pause = beat.interval;
                    assignment.send_replace(beat);
                    if beat.status == HeartbeatStatus::Stop {
                        halt.stop();
                    }
                }
                Err(failure) if halt.stops_on(&failure) => return,
                Err(failure) => {
                    let code = failure.code().as_str();
                    let event = EVENT_FAILED;
                    tracing::warn!(
                        error_code = code,
                        event,
                        "a heartbeat failed; the last assignment stands"
                    );
                    pause = retries
                        .next()
                        .unwrap_or(self.last.interval)
                        .min(self.last.interval);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "heartbeat/tests.rs"]
mod tests;
