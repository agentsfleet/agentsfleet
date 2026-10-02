//! Sandboxes started before their lease, so a lease start skips the wait.
//!
//! [`WarmSlots`] wraps any [`Engine`] and is one itself. A single task owns the
//! ready sandboxes; a lease claims one through a channel and the task starts a
//! replacement, so no lock guards the pool and no sandbox is handed out twice.
//! A claimed sandbox is destroyed with its lease and never returns.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afd_core::error_code::{self, ErrorCode};
use backon::{ExponentialBuilder, Retryable as _, Sleeper};

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
/// The event a slot whose sandbox died while waiting is logged under.
const EVENT_SLOT_DIED: &str = "sandbox_warm_slot_died";
/// The code a slot that died, or a keeper that stopped, is logged under.
const DIED_CODE: ErrorCode = error_code::INTERNAL_OPERATION_FAILED;
/// The first wait after a failed start.
const RETRY_MIN_DELAY: Duration = Duration::from_millis(100);
/// The longest wait between two starts.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
/// How a start built on demand is labelled.
const COLD: &str = "cold";

/// A failed start: the slot's name, and why.
type Failed = (String, crate::Error);

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
            tracing::warn!(
                error_code = DIED_CODE.as_str(),
                reason,
                event,
                "the warm-slot keeper stopped abnormally"
            );
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
    // Every start and every retirement in flight, owned here so shutdown can
    // wait for each one: a start abandoned mid-way leaves its cgroup behind.
    let mut work = JoinSet::new();
    let refill = |work: &mut JoinSet<()>| {
        work.spawn(fill(Arc::clone(&inner), ready.clone(), limits));
    };
    for _ in 0..slots {
        refill(&mut work);
    }
    while let Some(reply) = requests.recv().await {
        while work.try_join_next().is_some() {}
        let slot = live_slot(&mut waiting, &mut work);
        if slot.is_some() {
            refill(&mut work);
        }
        if let Err(Some(unclaimed)) = reply.send(slot) {
            // The lease stopped waiting; this slot is still unused but no
            // longer counted, so it goes the way every slot goes.
            work.spawn(retire(unclaimed));
        }
    }
    // Closed first, so a start still in flight stops retrying and retires its
    // own sandbox rather than handing it to nobody.
    waiting.close();
    while let Some(slot) = waiting.recv().await {
        retire(slot).await;
    }
    while work.join_next().await.is_some() {}
}

/// The next ready slot whose sandbox is still up. One that died while it
/// waited is retired and replaced, never handed to a lease.
fn live_slot(
    waiting: &mut mpsc::Receiver<Box<dyn Sandbox>>,
    work: &mut JoinSet<()>,
) -> Option<Box<dyn Sandbox>> {
    loop {
        let mut slot = waiting.try_recv().ok()?;
        if slot.is_running() {
            return Some(slot);
        }
        let event = EVENT_SLOT_DIED;
        tracing::warn!(
            error_code = DIED_CODE.as_str(),
            event,
            "a warm slot's sandbox died while it waited"
        );
        work.spawn(retire(slot));
    }
}

/// Starts one slot and offers it to the keeper, retrying a failed start with
/// backoff for as long as the keeper runs: a host that refused one start is
/// not thereby a host with fewer slots for good. A closed keeper retires it.
async fn fill(inner: Arc<dyn Engine>, ready: mpsc::Sender<Box<dyn Sandbox>>, limits: Limits) {
    let attempt = || {
        let inner = Arc::clone(&inner);
        async move {
            // Named like a lease, so a slot never meets a leftover of a
            // previous run under the same name.
            let lease_id = format!("{SLOT_PREFIX}{}", uuid::Uuid::now_v7());
            inner
                .prepare(SandboxRequest {
                    lease_id: &lease_id,
                    limits,
                })
                .await
                .map_err(|error| (lease_id, error))
        }
    };
    let started = attempt
        .retry(
            ExponentialBuilder::default()
                .with_min_delay(RETRY_MIN_DELAY)
                .with_max_delay(RETRY_MAX_DELAY)
                .without_max_times(),
        )
        .sleep(UntilClosed(ready.clone()))
        .when(|_failed: &Failed| !ready.is_closed())
        .notify(|(lease_id, error): &Failed, delay: Duration| {
            let error_code = error.code().as_str();
            let reason = error.to_string();
            let retry_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
            let event = EVENT_SLOT_FAILED;
            tracing::warn!(lease_id, error_code, reason, retry_ms, event);
        })
        .await;
    if let Ok(sandbox) = started
        && let Err(closed) = ready.send(sandbox).await
    {
        retire(closed.0).await;
    }
}

/// A retry's wait, cut short when the keeper shuts down, so shutdown never
/// waits out a backoff.
struct UntilClosed(mpsc::Sender<Box<dyn Sandbox>>);

impl Sleeper for UntilClosed {
    type Sleep = Pin<Box<dyn Future<Output = ()> + Send>>;

    fn sleep(&self, dur: Duration) -> Self::Sleep {
        let keeper = self.0.clone();
        Box::pin(async move {
            tokio::select! {
                () = tokio::time::sleep(dur) => {}
                () = keeper.closed() => {}
            }
        })
    }
}

/// Destroys a slot no lease will use.
async fn retire(slot: Box<dyn Sandbox>) {
    if let Err(error) = slot.destroy().await {
        let error_code = error.code().as_str();
        let reason = error.to_string();
        let event = EVENT_SLOT_LEFT;
        tracing::warn!(
            error_code,
            reason,
            event,
            "an unused warm slot failed to tear down"
        );
    }
}

#[cfg(test)]
mod tests;
