//! Liveness: the beat that carries what this host can do and which fleets'
//! sandboxes it holds, and brings back what it is assigned and which holds to
//! give up.

use std::borrow::Cow;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::limits::WorkerCount;
use afd_core::timing::HEARTBEAT_INTERVAL_MS;
use afd_wire::runner::{
    HeartbeatRequest, HeartbeatResponse, HeartbeatStatus, HeldFleets, NetworkPolicy, SandboxTier,
};
use afr_sandbox::HostProbe;
use tokio::sync::watch;

use crate::capability::{capability_report, selftest};
use crate::client::{ControlPlane, endless};
use crate::error::Result;
use crate::halt::Halt;
use crate::holds::{Holds, Release};

/// The shortest pause between beats, whatever the daemon asks: a reply saying
/// zero must not turn the heartbeat into a busy loop.
pub(crate) const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_FAILED: &str = "heartbeat_failed";
/// The event a fleet the daemon named for release, that is no fleet id, is
/// logged under.
const EVENT_RELEASE_UNREADABLE: &str = "sandbox_hold_release_unreadable";

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
    holds: &'a Holds,
    label: Option<(SandboxTier, NetworkPolicy)>,
    selftest_due: bool,
    last: Assignment,
}

impl<'a> Heartbeat<'a> {
    /// A beat over `plane`, reporting `probe` and what `holds` holds.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        probe: &'a HostProbe,
        holds: &'a Holds,
    ) -> Self {
        Self {
            plane,
            probe,
            holds,
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
        let held = self.holds.fleets().await;
        let request = HeartbeatRequest {
            capability_report: Some(capability_report(self.probe)),
            selftest,
            holds: HeldFleets(
                held.iter()
                    .map(|fleet| Cow::Borrowed(fleet.as_str()))
                    .collect(),
            ),
        };
        let body = self.plane.heartbeat(&request).await?;
        let reply: HeartbeatResponse<'_> = body.decode()?;
        for inactive in &reply.release_holds {
            match Uuid7::parse(inactive) {
                Ok(fleet) => self.holds.release(fleet, Release::Inactive),
                Err(failure) => unreadable_release(inactive, failure),
            }
        }
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
    /// again, sooner the first time and backing off after. Holds released
    /// because no worker was free beat at once, so the daemon stops routing
    /// those fleets here without waiting out the interval.
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
                () = self.holds.saturated().notified() => {}
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

/// Logs a fleet the daemon named for release that is no fleet id, under the
/// code the runner gives any identifier it cannot read. No hold is released:
/// nothing names one.
fn unreadable_release(named: &str, failure: afd_core::error::Error) {
    let reason = failure.to_string();
    let error_code = crate::Error::from(failure).code().as_str();
    let fleet_id = named;
    let event = EVENT_RELEASE_UNREADABLE;
    tracing::warn!(error_code, fleet_id, reason, event);
}

#[cfg(test)]
#[path = "heartbeat/tests.rs"]
mod tests;
