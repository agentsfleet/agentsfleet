//! Host liveness, pushed assignments and parked-sandbox releases share one policy owner.

use std::{borrow::Cow, path::Path, sync::Arc, time::Duration};

use af_core::{id::Uuid7, limits::WorkerCount};
use af_stream::OwnedFrame;
use af_wire::{
    protocol::Down,
    runner::{
        HeartbeatRequest, HeartbeatResponse, HeartbeatStatus, NetworkPolicy, ParkedFleets,
        SandboxTier,
    },
};
use afr_engine::{Capabilities, HostProbe};
use tokio::sync::watch;

use crate::{
    capability::{HostReader, capability_report, selftest},
    client::ControlPlane,
    egress::Egress,
    error::{ErrorKind, Result},
    halt::Halt,
    parks::{Parks, Release},
};

const EVENT_LAST_FAILED: &str = "heartbeat_last_failed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub(crate) status: HeartbeatStatus,
    pub(crate) workers: u32,
    pub(crate) egress: Egress,
}

impl Assignment {
    pub(crate) fn initial() -> Self {
        Self {
            status: HeartbeatStatus::Ok,
            workers: 0,
            egress: Egress::closed(),
        }
    }

    pub(crate) const fn takes_work(&self, worker: u32) -> bool {
        matches!(self.status, HeartbeatStatus::Ok) && worker < self.workers
    }
}

#[derive(Debug)]
pub(crate) struct Heartbeat<'a> {
    plane: &'a ControlPlane,
    probe: &'a HostProbe,
    /// What the runner's engine states, which the report and self-test name.
    engine: Capabilities,
    /// The engine's admission bound, which the report states.
    sandboxes_max: Option<u32>,
    parks: &'a Parks,
    /// Where run sandboxes live, whose file system's free bytes each beat reports.
    sandboxes: &'a Path,
    host: HostReader,
    label: Option<(SandboxTier, NetworkPolicy)>,
    selftest_due: bool,
    last: Assignment,
    closed: bool,
}

impl<'a> Heartbeat<'a> {
    pub(crate) fn new(
        plane: &'a ControlPlane,
        probe: &'a HostProbe,
        engine: Capabilities,
        parks: &'a Parks,
        sandboxes: &'a Path,
    ) -> Self {
        Self {
            plane,
            probe,
            engine,
            sandboxes_max: None,
            parks,
            sandboxes,
            host: HostReader::default(),
            label: None,
            selftest_due: false,
            last: Assignment::initial(),
            closed: false,
        }
    }

    /// The same heartbeat, stating `sandboxes_max` as the engine's memory
    /// bound.
    pub(crate) const fn with_sandboxes_max(mut self, sandboxes_max: Option<u32>) -> Self {
        self.sandboxes_max = sandboxes_max;
        self
    }

    pub(crate) async fn beat(&mut self) -> Result<Assignment> {
        let selftest = self
            .label
            .filter(|_| self.selftest_due)
            .map(|(tier, network)| selftest(self.probe, &self.engine, tier, network));
        let parked = self.parks.fleets().await;
        let host = self.host.facts(self.sandboxes).await;
        let request = HeartbeatRequest {
            capability_report: Some(capability_report(
                self.probe,
                &self.engine,
                self.sandboxes_max,
            )),
            selftest,
            parked: ParkedFleets(
                parked
                    .iter()
                    .map(|fleet| Cow::Borrowed(fleet.as_str()))
                    .collect(),
            ),
            closing: self.closed,
            host,
        };
        let body = self.plane.heartbeat(&request).await?;
        self.apply(&body.decode::<HeartbeatResponse<'_>>()?)
    }

    fn apply(&mut self, reply: &HeartbeatResponse<'_>) -> Result<Assignment> {
        for inactive in &reply.release_parked {
            self.parks
                .release(Uuid7::parse(inactive)?, Release::Inactive);
        }
        let policy = &reply.assigned_policy;
        self.selftest_due = reply.selftest_requested;
        self.label = Some((policy.sandbox_tier, policy.network_policy));
        self.last = Assignment {
            status: reply.status,
            workers: if reply.degraded {
                0
            } else {
                WorkerCount::clamping(policy.worker_count).get()
            },
            egress: Egress::assigned(policy),
        };
        Ok(self.last.clone())
    }

    pub(crate) async fn keep_beating(
        mut self,
        assignment: &watch::Sender<Assignment>,
        halt: &Halt,
    ) {
        let mut updates = self.plane.stream.policies.clone();
        let mut retries = self.plane.runtime.endless();
        let rounds = Arc::clone(&self.plane.stream.renewing);
        // The first beat goes at once; after it the runner's clock wakes each beat, and a
        // failed beat retries on its own backoff.
        let mut backoff = Some(Duration::ZERO);
        loop {
            tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                () = halt.taking_runs().cancelled(), if !self.closed => { self.close(); }
                changed = updates.changed() => {
                    if changed.is_err() { halt.stop(); return; }
                    let frame = updates.borrow_and_update().clone();
                    if let Some(frame) = frame {
                        match pushed(&frame).and_then(|response| self.apply(&response)) {
                            Ok(policy) => { publish(assignment, policy, halt); }
                            Err(error) => return refused(&error, halt),
                        }
                    }
                    continue;
                }
                () = self.parks.saturated().notified() => {}
                () = rounds.beats().notified() => {}
                () = tokio::time::sleep(backoff.unwrap_or_default()), if backoff.is_some() => {}
            }
            let beaten = tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                beaten = self.beat() => beaten,
            };
            match beaten {
                Ok(policy) => {
                    retries = self.plane.runtime.endless();
                    publish(assignment, policy, halt);
                    backoff = match overtaken(&mut updates) {
                        Ok(overtaken) => overtaken.then_some(Duration::ZERO),
                        Err(error) => return refused(&error, halt),
                    };
                }
                Err(error) if halt.stops_on(&error) => return self.close(),
                Err(error) if !error.is_retryable() => {
                    let error_code = error.code().as_str();
                    tracing::error!(event = "heartbeat_failed", error_code);
                    halt.stop();
                    return self.close();
                }
                Err(error) => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "heartbeat_failed", error_code);
                    // The next round wakes the beat sooner than a long pause, and a new
                    // connection rounds at once.
                    let Some(pause) = retries.next() else {
                        halt.stop();
                        return;
                    };
                    backoff = Some(pause);
                }
            }
        }
    }

    fn close(&mut self) {
        self.closed = true;
        self.parks.close();
    }

    async fn last_beat(&mut self, halt: &Halt) {
        self.close();
        if halt.token_refused() {
            return;
        }
        // The stream's answer timeout bounds the beat, so a stop never waits
        // on a daemon that does not answer.
        let code = match self.beat().await {
            Ok(_) => return,
            Err(failure) if halt.stops_on(&failure) => return,
            Err(failure) => failure.code(),
        };
        let error_code = code.as_str();
        tracing::warn!(event = EVENT_LAST_FAILED, error_code);
    }
}

/// The policy a pushed frame carries.
fn pushed(frame: &OwnedFrame) -> Result<HeartbeatResponse<'_>> {
    match frame.decode::<Down<'_>>() {
        Ok(Down::Policy { response }) => Ok(response),
        Ok(_) => Err(ErrorKind::StreamProtocol.into()),
        Err(error) => Err(error.into()),
    }
}

/// Whether a policy was pushed while a beat was in flight.
///
/// Nothing on a frame orders it against that beat's answer: the daemon may have read the
/// host's row before the beat changed it, or after. The frame is marked seen and left
/// unapplied, and the caller beats again at once. That answer is newer than both.
///
/// # Errors
/// The frame is not a policy.
fn overtaken(updates: &mut watch::Receiver<Option<Arc<OwnedFrame>>>) -> Result<bool> {
    if !updates.has_changed().unwrap_or(false) {
        return Ok(false);
    }
    let frame = updates.borrow_and_update().clone();
    frame.map_or(Ok(false), |frame| pushed(&frame).map(|_policy| true))
}

/// Stops the runner on a pushed frame it cannot take as a policy.
fn refused(error: &crate::error::Error, halt: &Halt) {
    let error_code = error.code().as_str();
    tracing::error!(event = "runner_assignment_refused", error_code);
    halt.stop();
}

fn publish(assignment: &watch::Sender<Assignment>, policy: Assignment, halt: &Halt) {
    let stop = policy.status == HeartbeatStatus::Stop;
    assignment.send_replace(policy);
    if stop {
        halt.stop();
    }
}

#[cfg(test)]
mod clock_tests;
#[cfg(test)]
mod closing_tests;
#[cfg(test)]
mod durable_policy_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod last_beat_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod release_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod unanswered_tests;
