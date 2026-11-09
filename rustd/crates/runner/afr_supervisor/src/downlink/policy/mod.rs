//! Host liveness, pushed assignments and held-sandbox releases share one policy owner.

use std::{borrow::Cow, time::Duration};

use af_core::{id::Uuid7, limits::WorkerCount, timing::HEARTBEAT_INTERVAL};
use af_wire::{
    protocol::Down,
    runner::{
        HeartbeatRequest, HeartbeatResponse, HeartbeatStatus, HeldFleets, NetworkPolicy,
        SandboxTier,
    },
};
use afr_sandbox::HostProbe;
use tokio::sync::watch;

use crate::{
    capability::{capability_report, selftest},
    client::{ControlPlane, Verb, endless},
    egress::Egress,
    error::{ErrorKind, Result},
    halt::Halt,
    holds::{Holds, Release},
};

pub(crate) const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_LAST_FAILED: &str = "heartbeat_last_failed";
pub(crate) const LAST_BEAT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub(crate) status: HeartbeatStatus,
    pub(crate) workers: u32,
    pub(crate) interval: Duration,
    pub(crate) egress: Egress,
}

impl Assignment {
    pub(crate) fn initial() -> Self {
        Self {
            status: HeartbeatStatus::Ok,
            workers: 0,
            interval: HEARTBEAT_INTERVAL,
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
    holds: &'a Holds,
    label: Option<(SandboxTier, NetworkPolicy)>,
    selftest_due: bool,
    last: Assignment,
    closed: bool,
}

impl<'a> Heartbeat<'a> {
    pub(crate) fn new(plane: &'a ControlPlane, probe: &'a HostProbe, holds: &'a Holds) -> Self {
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
        self.apply(&body.decode::<HeartbeatResponse<'_>>()?)
    }

    fn apply(&mut self, reply: &HeartbeatResponse<'_>) -> Result<Assignment> {
        for inactive in &reply.release_holds {
            self.holds
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
            interval: Duration::from_millis(u64::from(reply.heartbeat_interval_ms))
                .max(MIN_HEARTBEAT_INTERVAL),
            egress: Egress::assigned(policy),
        };
        Ok(self.last.clone())
    }

    pub(crate) async fn keep_beating(
        mut self,
        assignment: &watch::Sender<Assignment>,
        halt: &Halt,
    ) {
        let mut updates = self.plane.stream.assignments.clone();
        let mut retries = endless();
        let mut pause = Duration::ZERO;
        loop {
            tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                () = halt.leasing().cancelled(), if !self.closed => { self.close(); }
                updated = updates.changed() => {
                    if updated.is_err() { halt.stop(); return; }
                    let frame = updates.borrow_and_update().clone();
                    if let Some(frame) = frame {
                        let result = match frame.decode::<Down<'_>>() {
                            Ok(Down::Assignment { response }) => self.apply(&response),
                            Ok(_) => Err(ErrorKind::StreamProtocol.into()),
                            Err(error) => Err(error.into()),
                        };
                        match result {
                            Ok(policy) => { publish(assignment, policy, halt); }
                            Err(error) => {
                                let error_code = error.code().as_str();
                                tracing::error!(event = "runner_assignment_refused", error_code);
                                halt.stop(); return;
                            }
                        }
                    }
                    continue;
                }
                () = self.holds.saturated().notified() => {}
                () = tokio::time::sleep(pause) => {}
            }
            let beaten = tokio::select! {
                biased;
                () = halt.serving().cancelled() => return self.last_beat(halt).await,
                beaten = self.beat() => beaten,
            };
            match beaten {
                Ok(policy) => {
                    retries = endless();
                    pause = policy.interval;
                    publish(assignment, policy, halt);
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
                    pause = match retries.next() {
                        Some(pause) => pause.min(self.last.interval),
                        None => {
                            halt.stop();
                            return;
                        }
                    };
                }
            }
        }
    }

    fn close(&mut self) {
        self.closed = true;
        self.holds.close();
    }

    async fn last_beat(&mut self, halt: &Halt) {
        self.close();
        if halt.token_refused() {
            return;
        }
        let code = match tokio::time::timeout(LAST_BEAT_TIMEOUT, self.beat()).await {
            Ok(Ok(_)) => return,
            Ok(Err(failure)) if halt.stops_on(&failure) => return,
            Ok(Err(failure)) => failure.code(),
            Err(_source) => Verb::Heartbeat.code(),
        };
        let error_code = code.as_str();
        tracing::warn!(event = EVENT_LAST_FAILED, error_code);
    }
}

fn publish(assignment: &watch::Sender<Assignment>, policy: Assignment, halt: &Halt) {
    let stop = policy.status == HeartbeatStatus::Stop;
    assignment.send_replace(policy);
    if stop {
        halt.stop();
    }
}

#[cfg(test)]
mod closing_tests;
#[cfg(test)]
mod durable_policy_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod unanswered_tests;
