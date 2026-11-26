//! Sandboxes started before their run, so a run start skips the wait.
//!
//! [`WarmSlots`] wraps any [`Engine`] and is one itself. A single task owns the
//! ready sandboxes; a run claims one through a channel and the task starts a
//! replacement, so no lock guards the pool and no sandbox is handed out twice.
//! A claimed sandbox is destroyed with its run and never returns.
//!
//! Every engine fixes a sandbox's kind of network when it builds, so slots are
//! built for the kind the daemon assigned this runner, which arrives after boot
//! through [`Engine::network_assigned`]: none is built before it, and a new
//! kind retires the ready slots and builds for itself. An allowlisted slot is
//! built to an empty allowlist, its host-side set empty, so it reaches nothing
//! until a claim applies its run's plan, which reads back in force before the
//! run is handed the sandbox.

use std::sync::Arc;
use std::time::Instant;

use af_core::clock::saturating_millis;
use af_core::error_code::{self, Coded as _, ErrorCode, Logged};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use self::keeper::{keep, retire};
use crate::engine::{
    Capabilities, EgressPlan, Engine, Granted, Limits, MemoryBound, Sandbox, SandboxSpec, Sweep,
    Swept,
};
use crate::error::Result;
use crate::network::NetworkKind;

mod keeper;

/// The event every run start is logged under, warm or cold.
const EVENT_START: &str = "sandbox_start_completed";
/// The event a keeper that stopped abnormally is logged under.
const EVENT_KEEPER_FAILED: &str = "sandbox_warm_keeper_failed";
/// The event a slot that refused its run's plan is logged under.
const EVENT_SLOT_UNPLANNED: &str = "sandbox_warm_slot_unplanned";
/// The code a slot that died, or a keeper that stopped, is logged under.
const DIED_CODE: ErrorCode = error_code::INTERNAL_OPERATION_FAILED;
/// How a start served from a slot is labelled.
const WARM: &str = "warm";
/// How a start built on demand is labelled.
const COLD: &str = "cold";

/// A run's request for a ready slot built for its kind of network, answered
/// `None` when none is ready.
type Claim = (NetworkKind, oneshot::Sender<Option<Slot>>);

/// A sandbox started ahead of its run, under an identifier of its own that
/// its cgroup and its directory carry, so a run served from it logs it.
struct Slot {
    sandbox: Box<dyn Sandbox>,
    /// The kind of network it was built for.
    kind: NetworkKind,
}

/// An engine that keeps `slots` sandboxes started ahead of their runs.
#[derive(Debug)]
pub struct WarmSlots {
    inner: Arc<dyn Engine>,
    limits: Limits,
    /// How many sandboxes it keeps ready, each holding a reservation.
    slots: usize,
    claims: mpsc::Sender<Claim>,
    /// The kind of network the daemon assigned; none before its first
    /// assignment.
    kind: watch::Sender<Option<NetworkKind>>,
    keeper: JoinHandle<()>,
}

impl WarmSlots {
    /// Keeps `slots` sandboxes with `limits` from `inner` ready, each built
    /// once a kind of network is assigned. Zero slots makes every start cold.
    pub fn start(inner: Arc<dyn Engine>, slots: usize, limits: Limits) -> Self {
        let (claims, requests) = mpsc::channel(1);
        let (kind, assigned) = watch::channel(None);
        let keeper = tokio::spawn(keep(
            Arc::clone(&inner),
            (requests, assigned),
            slots,
            limits,
        ));
        Self {
            inner,
            limits,
            slots,
            claims,
            kind,
            keeper,
        }
    }

    /// Stops refilling, waits for every start in flight, and destroys every
    /// slot no run claimed.
    pub async fn shutdown(self) {
        drop(self.claims);
        if let Err(stopped) = self.keeper.await {
            let reason = stopped.to_string();
            let event = EVENT_KEEPER_FAILED;
            let error_code = DIED_CODE.as_str();
            {
                let message = format_args!("the warm-slot keeper stopped abnormally");
                tracing::warn!(message, error_code, reason, event);
            };
        }
    }

    async fn claim(&self, kind: NetworkKind) -> Option<Slot> {
        let (reply, answer) = oneshot::channel();
        self.claims.send((kind, reply)).await.ok()?;
        answer.await.ok().flatten()
    }
}

#[async_trait::async_trait]
impl Engine for WarmSlots {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        self.inner.grant(limits)
    }

    /// The inner engine's bound less the slots kept ready: each holds a
    /// reservation, so the runs offered beside them always fit a cold build,
    /// and the slots plus live runs never pass the bound.
    fn memory_bound(&self) -> Option<MemoryBound> {
        self.inner.memory_bound().map(|bound| MemoryBound {
            sandboxes_max: bound
                .sandboxes_max
                .saturating_sub(u32::try_from(self.slots).unwrap_or(u32::MAX)),
            ..bound
        })
    }

    /// Builds every slot for `kind` from now on.
    fn network_assigned(&self, kind: NetworkKind) {
        self.kind
            .send_if_modified(|held| held.replace(kind) != Some(kind));
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        let started = Instant::now();
        let warm = if *spec.granted.limits() == self.limits {
            self.claim(spec.egress.network.kind()).await
        } else {
            None
        };
        let planned = match warm {
            Some(slot) => planned(slot, spec.egress).await,
            None => None,
        };
        let (sandbox, start) = match planned {
            Some(sandbox) => (sandbox, WARM),
            None => (self.inner.prepare(spec).await?, COLD),
        };
        let sandbox_id = sandbox.id().as_str();
        let duration_ms = saturating_millis(started.elapsed());
        let event = EVENT_START;
        tracing::info!(sandbox_id, start, duration_ms, event);
        Ok(sandbox)
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        self.inner.sweep(scope).await
    }
}

/// `slot`'s sandbox held to its run's `plan`, read back in force; none when
/// it refused, the slot then destroyed so the cold build fits the bound.
async fn planned(slot: Slot, plan: EgressPlan<'_>) -> Option<Box<dyn Sandbox>> {
    let Slot { mut sandbox, kind } = slot;
    let Err(error) = sandbox.egress(plan).await else {
        return Some(sandbox);
    };
    let Logged { error_code, reason } = error.logged();
    let event = EVENT_SLOT_UNPLANNED;
    {
        let sandbox_id = sandbox.id().as_str();
        let message = format_args!("a warm slot refused its run's plan; the run starts cold");
        tracing::warn!(message, error_code, sandbox_id, reason, event);
    };
    retire(Slot { sandbox, kind }).await;
    None
}

#[cfg(test)]
mod tests;
