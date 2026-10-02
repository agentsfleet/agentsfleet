//! Sandboxes started before their lease, so a lease start skips the wait.
//!
//! [`WarmSlots`] wraps any [`Engine`] and is one itself. A single task owns the
//! ready sandboxes; a lease claims one through a channel and the task starts a
//! replacement, so no lock guards the pool and no sandbox is handed out twice.
//! A claimed sandbox is destroyed with its lease and never returns.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

use crate::engine::{Engine, Limits, Sandbox, SandboxRequest};
use crate::error::Result;

/// What every warm sandbox is named, before its lease is known.
const SLOT_PREFIX: &str = "warm-";
/// The event every lease start is logged under, warm or cold.
const EVENT_START: &str = "sandbox_start_ms";
/// The event a slot that failed to start is logged under.
const EVENT_SLOT_FAILED: &str = "sandbox_warm_slot_failed";
/// The event a slot that failed to tear down is logged under.
const EVENT_SLOT_LEFT: &str = "sandbox_warm_slot_left";
/// How a start served from a slot is labelled.
const WARM: &str = "warm";
/// How a start built on demand is labelled.
const COLD: &str = "cold";

/// A lease's request for a ready sandbox; `None` when none is ready.
type Claim = oneshot::Sender<Option<Box<dyn Sandbox>>>;

/// An engine that keeps `slots` sandboxes started ahead of their leases.
#[derive(Debug)]
pub struct WarmSlots {
    inner: Arc<dyn Engine>,
    limits: Limits,
    claims: mpsc::Sender<Claim>,
    keeper: JoinHandle<()>,
}

impl WarmSlots {
    /// Starts `slots` sandboxes with `limits` from `inner`, and keeps that many
    /// ready. Zero slots makes every start cold.
    pub fn start(inner: Arc<dyn Engine>, slots: usize, limits: Limits) -> Self {
        let (claims, requests) = mpsc::channel(1);
        let keeper = tokio::spawn(keep(Arc::clone(&inner), requests, slots, limits));
        Self {
            inner,
            limits,
            claims,
            keeper,
        }
    }

    /// Stops refilling, waits for every start in flight, and destroys every
    /// slot no lease claimed.
    pub async fn shutdown(self) {
        drop(self.claims);
        if let Err(stopped) = self.keeper.await {
            let reason = stopped.to_string();
            let event = EVENT_SLOT_LEFT;
            tracing::warn!(reason, event, "the warm-slot keeper stopped abnormally");
        }
    }

    async fn claim(&self) -> Option<Box<dyn Sandbox>> {
        let (reply, answer) = oneshot::channel();
        self.claims.send(reply).await.ok()?;
        answer.await.ok().flatten()
    }
}

#[async_trait::async_trait]
impl Engine for WarmSlots {
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        let started = Instant::now();
        let warm = if request.limits == self.limits {
            self.claim().await
        } else {
            None
        };
        let (sandbox, start) = match warm {
            Some(sandbox) => (sandbox, WARM),
            None => (self.inner.prepare(request).await?, COLD),
        };
        let lease_id = request.lease_id;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let event = EVENT_START;
        tracing::info!(lease_id, start, ms, event);
        Ok(sandbox)
    }
}

/// The task that owns every ready slot.
async fn keep(
    inner: Arc<dyn Engine>,
    mut requests: mpsc::Receiver<Claim>,
    slots: usize,
    limits: Limits,
) {
    let (ready, mut waiting) = mpsc::channel(slots.max(1));
    // Every start in flight, owned here so shutdown can wait for each one: a
    // start abandoned mid-way leaves its cgroup and disk behind.
    let mut fills = JoinSet::new();
    let mut started = 0_u64;
    let mut refill = |fills: &mut JoinSet<()>| {
        started += 1;
        fills.spawn(fill(Arc::clone(&inner), ready.clone(), started, limits));
    };
    for _ in 0..slots {
        refill(&mut fills);
    }
    while let Some(reply) = requests.recv().await {
        while fills.try_join_next().is_some() {}
        let slot = waiting.try_recv().ok();
        if slot.is_some() {
            refill(&mut fills);
        }
        if let Err(Some(unclaimed)) = reply.send(slot) {
            // The lease stopped waiting; this slot is still unused but no
            // longer counted, so it goes the way every slot goes.
            retire(unclaimed).await;
        }
    }
    // Closed first, so a start still in flight retires its own sandbox rather
    // than handing it to nobody.
    waiting.close();
    while let Some(slot) = waiting.recv().await {
        retire(slot).await;
    }
    while fills.join_next().await.is_some() {}
}

/// Starts one slot and offers it to the keeper; a closed keeper retires it.
async fn fill(
    inner: Arc<dyn Engine>,
    ready: mpsc::Sender<Box<dyn Sandbox>>,
    number: u64,
    limits: Limits,
) {
    let lease_id = format!("{SLOT_PREFIX}{number}");
    match inner
        .prepare(SandboxRequest {
            lease_id: &lease_id,
            limits,
        })
        .await
    {
        Ok(sandbox) => {
            if let Err(closed) = ready.send(sandbox).await {
                retire(closed.0).await;
            }
        }
        Err(error) => {
            let reason = error.to_string();
            let event = EVENT_SLOT_FAILED;
            tracing::warn!(lease_id, reason, event, "a warm slot failed to start");
        }
    }
}

/// Destroys a slot no lease will use.
async fn retire(slot: Box<dyn Sandbox>) {
    if let Err(error) = slot.destroy().await {
        let reason = error.to_string();
        let event = EVENT_SLOT_LEFT;
        tracing::warn!(reason, event, "an unused warm slot failed to tear down");
    }
}

#[cfg(test)]
mod tests;
