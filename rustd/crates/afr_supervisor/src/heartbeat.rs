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
use crate::client::{ControlPlane, Verb, endless};
use crate::error::Result;
use crate::halt::Halt;
use crate::holds::{Holds, Release};

/// The shortest pause between beats, whatever the daemon asks: a reply saying
/// zero must not turn the heartbeat into a busy loop.
pub(crate) const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_FAILED: &str = "heartbeat_failed";
/// The event a runner's last beat, which lists no holds, failing is logged
/// under.
const EVENT_LAST_FAILED: &str = "heartbeat_last_failed";
/// The event a fleet the daemon named for release, that is no fleet id, is
/// logged under.
const EVENT_RELEASE_UNREADABLE: &str = "sandbox_hold_release_unreadable";
/// How long a stopping runner waits on its last beat before giving up. A stop
/// must not wait out a whole call timeout on a daemon that does not answer:
/// the holds lapse there either way, once it finds this runner silent.
pub(crate) const LAST_BEAT_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Whether the holds have closed, which makes every list from here on
    /// final: nothing is held, and nothing more is parked.
    closed: bool,
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
            closed: false,
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
            closing: self.closed,
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
    ///
    /// Once leasing stops no lease could take a hold, so every hold ends and
    /// the beat goes at once, saying its list is final. Leasing is a child of
    /// serving, so a runner that stops serving gets there too; it says so in
    /// [`Heartbeat::last_beat`], abandoning a beat still in flight.
    pub(crate) async fn keep_beating(
        mut self,
        assignment: &watch::Sender<Assignment>,
        halt: &Halt,
    ) {
        let mut retries = endless();
        let mut pause = Duration::ZERO;
        loop {
            tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                () = halt.leasing().cancelled(), if !self.closed => self.close(),
                () = tokio::time::sleep(pause) => {}
                () = self.holds.saturated().notified() => {}
            }
            // A daemon that does not answer must not hold up a stop for a
            // whole call timeout: the beat is dropped for the last one.
            let beaten = tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                beaten = self.beat() => beaten,
            };
            match beaten {
                Ok(beat) => {
                    retries = endless();
                    pause = beat.interval;
                    assignment.send_replace(beat);
                    if beat.status == HeartbeatStatus::Stop {
                        halt.stop();
                    }
                }
                // No last beat: with the token refused, no call can succeed.
                Err(failure) if halt.stops_on(&failure) => return self.close(),
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

    /// Ends every hold and parks nothing more, so each beat from here on
    /// says its list is final.
    fn close(&mut self) {
        self.closed = true;
        self.holds.close();
    }

    /// Ends every hold and beats once more listing none, so the daemon routes
    /// their fleets elsewhere now rather than once it finds this runner
    /// silent. Best effort, and its answer is not acted on: the runner stops
    /// either way, and a refused token means no call can succeed. A daemon
    /// that has not answered within [`LAST_BEAT_TIMEOUT`] is given up on.
    async fn last_beat(&mut self, halt: &Halt) {
        self.close();
        if halt.token_refused() {
            return;
        }
        let code = match tokio::time::timeout(LAST_BEAT_TIMEOUT, self.beat()).await {
            Ok(Ok(_answered)) => return,
            Ok(Err(failure)) if halt.stops_on(&failure) => return,
            Ok(Err(failure)) => failure.code(),
            // Unanswered is how a heartbeat fails in transit, and is logged so.
            Err(_unanswered) => Verb::Heartbeat.code(),
        };
        let code = code.as_str();
        let event = EVENT_LAST_FAILED;
        tracing::warn!(
            error_code = code,
            event,
            "the last beat failed; this runner's holds stand at the daemon until they lapse"
        );
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

#[cfg(test)]
#[path = "heartbeat/closing_tests.rs"]
mod closing_tests;

#[cfg(test)]
#[path = "heartbeat/unanswered_tests.rs"]
mod unanswered_tests;
