//! Liveness: the beat that carries what this host can do and which fleets'
//! sandboxes it holds, and brings back what it is assigned and which holds to
//! give up.

use std::borrow::Cow;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::limits::WorkerCount;
use afd_core::timing::HEARTBEAT_INTERVAL;
use afd_wire::runner::{
    HeartbeatRequest, HeartbeatResponse, HeartbeatStatus, HeldFleets, NetworkPolicy, SandboxTier,
};
use afr_sandbox::HostProbe;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::capability::{capability_report, selftest};
use crate::client::{ControlPlane, Verb, endless};
use crate::egress::Egress;
use crate::error::Result;
use crate::halt::Halt;
use crate::holds::{Holds, Release};
use crate::worker_pool::{MIN_POLL_PAUSE, Wake};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assignment {
    /// Whether to take work, finish it, or stop.
    pub(crate) status: HeartbeatStatus,
    /// How many leases to run at once; zero until a policy arrives.
    pub(crate) workers: u32,
    /// How long until the next beat.
    pub(crate) interval: Duration,
    /// What each lease's sandbox reaches; nothing until a policy arrives.
    pub(crate) egress: Egress,
}

impl Assignment {
    /// What a runner assumes before its first beat is answered: no work, since
    /// it has no policy to work under, and no egress.
    pub(crate) fn initial() -> Self {
        Self {
            status: HeartbeatStatus::Ok,
            workers: 0,
            interval: HEARTBEAT_INTERVAL,
            egress: Egress::closed(),
        }
    }

    /// Whether worker number `worker` should take new work.
    pub(crate) const fn takes_work(&self, worker: u32) -> bool {
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
    /// The pool's idle workers, which a beat held until fresh work wakes.
    /// `None` never asks to be held.
    wake: Option<&'a Wake>,
    /// Whether a reply carried `woken`, the proof this daemon holds beats; an
    /// older daemon would read the field as no report at all.
    wake_proved: bool,
    /// How the last answered beat ended, which paces the next.
    last_beat: LastBeat,
}

/// How an answered beat ended, as the next beat's pause reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastBeat {
    /// It did not ask to be held.
    Unasked,
    /// It was held and answered without fresh work.
    Held,
    /// It was held and woken by fresh work.
    Woken,
}

impl<'a> Heartbeat<'a> {
    /// A beat over `plane`, reporting `probe` and what `holds` holds.
    pub(crate) fn new(plane: &'a ControlPlane, probe: &'a HostProbe, holds: &'a Holds) -> Self {
        Self {
            plane,
            probe,
            holds,
            label: None,
            selftest_due: false,
            last: Assignment::initial(),
            closed: false,
            wake: None,
            wake_proved: false,
            last_beat: LastBeat::Unasked,
        }
    }

    /// The same beat, asking to be held while one of `wake`'s workers is idle.
    pub(crate) const fn with_wake(mut self, wake: &'a Wake) -> Self {
        self.wake = Some(wake);
        self
    }

    /// Whether this beat asks the daemon to hold it until fresh work: the
    /// daemon has proved it holds beats, the runner takes work, and a worker
    /// is idle to take it. With every worker busy there is nothing to wake.
    fn asks(&self) -> bool {
        self.wake_proved
            && !self.closed
            && self.last.status == HeartbeatStatus::Ok
            && self.last.workers > 0
            && self.wake.is_some_and(Wake::any_idle)
    }

    /// Beats once, carrying a self-test when one was asked for.
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
            wake: self.asks(),
        };
        let asked = request.wake;
        let body = self.plane.heartbeat(&request).await?;
        let reply: HeartbeatResponse<'_> = body.decode()?;
        self.wake_proved = reply.woken.is_some();
        self.last_beat = match (asked, reply.woken) {
            (true, Some(true)) => LastBeat::Woken,
            (true, _) => LastBeat::Held,
            (false, _) => LastBeat::Unasked,
        };
        if let Some(wake) = self.wake.filter(|_| self.last_beat == LastBeat::Woken) {
            wake.one();
        }
        for inactive in &reply.release_holds {
            match Uuid7::parse(inactive) {
                Ok(fleet) => self.holds.release(fleet, Release::Inactive),
                Err(failure) => unreadable_release(inactive, failure),
            }
        }
        let policy = &reply.assigned_policy;
        self.selftest_due = reply.selftest_requested;
        self.label = Some((policy.sandbox_tier, policy.network_policy));
        self.last = Assignment {
            status: reply.status,
            workers: WorkerCount::clamping(policy.worker_count).get(),
            interval: Duration::from_millis(u64::from(reply.heartbeat_interval_ms))
                .max(MIN_HEARTBEAT_INTERVAL),
            egress: Egress::assigned(policy),
        };
        Ok(self.last.clone())
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
            let began = Instant::now();
            let holds = self.holds;
            let leasing = halt.leasing();
            let beaten = tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                // A release cannot wait out a held beat: that beat is dropped,
                // its writes already done, and sent again at once with the
                // holds as they now stand.
                () = holds.saturated().notified() => {
                    pause = Duration::ZERO;
                    continue;
                }
                // Nor can the final list: once leasing stops, the held beat is
                // dropped and the closing beat goes at once.
                () = leasing.cancelled(), if !self.closed => {
                    self.close();
                    pause = Duration::ZERO;
                    continue;
                }
                beaten = self.beat() => beaten,
            };
            match beaten {
                Ok(beat) => {
                    retries = endless();
                    pause = self.next_pause(beat.interval, began.elapsed());
                    let stop = beat.status == HeartbeatStatus::Stop;
                    assignment.send_replace(beat);
                    if stop {
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

    /// How long to wait before the next beat. A woken beat goes again after
    /// the shortest poll pause, so a burst of admissions cannot turn beats
    /// into a busy loop; a held beat already waited, so it waits only what is
    /// left of the interval; any other waits the interval.
    fn next_pause(&self, interval: Duration, took: Duration) -> Duration {
        match self.last_beat {
            LastBeat::Woken => MIN_POLL_PAUSE,
            LastBeat::Held => interval.saturating_sub(took),
            LastBeat::Unasked => interval,
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
    let reason = failure.told();
    let error_code = crate::Error::from(failure).code().as_str();
    let fleet_id = named;
    let event = EVENT_RELEASE_UNREADABLE;
    tracing::warn!(error_code, fleet_id, reason, event);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod closing_tests;

#[cfg(test)]
mod unanswered_tests;
