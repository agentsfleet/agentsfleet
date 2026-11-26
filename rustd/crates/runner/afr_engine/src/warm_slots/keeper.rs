//! The task that owns every ready slot, and the starts and retirements it
//! runs.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use af_core::clock::saturating_millis;
use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use backon::{ExponentialBuilder, Retryable as _, Sleeper};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use super::{Claim, DIED_CODE, Slot};
use crate::engine::{EgressPlan, Engine, Limits, SandboxSpec, SandboxState};
use crate::network::{Allowlist, Network, NetworkKind};

/// The event a slot that failed to start is logged under.
const EVENT_SLOT_FAILED: &str = "sandbox_warm_slot_failed";
/// The event a slot that failed to tear down is logged under.
const EVENT_SLOT_LEFT: &str = "sandbox_warm_slot_left";
/// The event a slot whose sandbox died while waiting is logged under.
const EVENT_SLOT_DIED: &str = "sandbox_warm_slot_died";
/// The first wait after a failed start.
const RETRY_MIN_DELAY: Duration = Duration::from_millis(100);
/// The longest wait between two starts.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(30);

/// A failed start: the slot's sandbox identifier, when one was minted, and
/// why.
type Failed = (Option<Uuid7>, crate::Error);

/// The task that owns every ready slot, answering `claims` and building for
/// each kind `assigned`.
pub(super) async fn keep(
    inner: Arc<dyn Engine>,
    (mut claims, mut assigned): (mpsc::Receiver<Claim>, watch::Receiver<Option<NetworkKind>>),
    slots: usize,
    limits: Limits,
) {
    let (ready, mut waiting) = mpsc::channel(slots.max(1));
    // Every start and every retirement in flight, owned here so shutdown can
    // wait for each one: a start abandoned mid-way leaves its cgroup behind.
    let mut work = JoinSet::new();
    let refill = |work: &mut JoinSet<()>, kind| {
        work.spawn(fill(Arc::clone(&inner), ready.clone(), limits, kind));
    };
    let mut current = None;
    loop {
        tokio::select! {
            claim = claims.recv() => {
                let Some(claim) = claim else { break };
                answer(claim, current, &mut waiting, &mut work, refill);
            }
            Ok(()) = assigned.changed() => {
                let Some(kind) = *assigned.borrow_and_update() else { continue };
                // The first kind fills the pool; a later one replaces each
                // ready slot. One of an old kind still starting is retired
                // when a claim or the next kind meets it.
                let replaced = if current.is_some() {
                    retire_ready(&mut waiting, &mut work)
                } else {
                    slots
                };
                current = Some(kind);
                for _ in 0..replaced {
                    refill(&mut work, kind);
                }
            }
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

/// Answers `claim` with a live slot when it asks for the `current` kind.
fn answer(
    (asked, reply): Claim,
    current: Option<NetworkKind>,
    waiting: &mut mpsc::Receiver<Slot>,
    work: &mut JoinSet<()>,
    refill: impl Fn(&mut JoinSet<()>, NetworkKind),
) {
    while work.try_join_next().is_some() {}
    let slot = current
        .filter(|kind| *kind == asked)
        .and_then(|kind| live_slot(waiting, work, |work| refill(work, kind), kind));
    if let Err(Some(unclaimed)) = reply.send(slot) {
        // The run stopped waiting; this slot is still unused but no longer
        // counted, so it goes the way every slot goes.
        work.spawn(retire(unclaimed));
    }
}

/// Retires every ready slot, counting them.
fn retire_ready(waiting: &mut mpsc::Receiver<Slot>, work: &mut JoinSet<()>) -> usize {
    let mut retired = 0;
    while let Ok(slot) = waiting.try_recv() {
        work.spawn(retire(slot));
        retired += 1;
    }
    retired
}

/// The next ready slot built for `kind` whose sandbox is still up. Every slot
/// taken is replaced, the one handed out and each one that died while it
/// waited or was built for an older kind, which is retired and never handed
/// to a run; so the pool keeps its size.
fn live_slot(
    waiting: &mut mpsc::Receiver<Slot>,
    work: &mut JoinSet<()>,
    refill: impl Fn(&mut JoinSet<()>),
    kind: NetworkKind,
) -> Option<Slot> {
    loop {
        let slot = waiting.try_recv().ok()?;
        refill(work);
        if slot.kind != kind {
            work.spawn(retire(slot));
            continue;
        }
        if slot.sandbox.state() == SandboxState::Running {
            return Some(slot);
        }
        let event = EVENT_SLOT_DIED;
        let error_code = DIED_CODE.as_str();
        let sandbox_id = slot.sandbox.id().as_str();
        {
            let message = format_args!("a warm slot's sandbox died while it waited");
            tracing::warn!(message, error_code, sandbox_id, event);
        };
        work.spawn(retire(slot));
    }
}

/// Starts one slot for `kind` and offers it to the keeper, retrying a failed
/// start with backoff for as long as the keeper runs: a host that refused one
/// start is not thereby a host with fewer slots for good. A closed keeper
/// retires it.
async fn fill(
    inner: Arc<dyn Engine>,
    ready: mpsc::Sender<Slot>,
    limits: Limits,
    kind: NetworkKind,
) {
    let empty = Allowlist::default();
    let network = match kind {
        NetworkKind::Host => Network::Host,
        NetworkKind::Isolated => Network::Isolated,
        NetworkKind::Allowed => Network::Allowed(&empty),
    };
    let attempt = || {
        let inner = Arc::clone(&inner);
        async move {
            // A fresh identifier per start, so a slot never meets a leftover
            // of a previous one.
            let id = Entropy::new()
                .uuid7(af_core::clock::now())
                .map_err(|error| (None, error.into()))?;
            let granted = inner
                .grant(limits)
                .map_err(|error| (Some(id.clone()), error))?;
            let spec = SandboxSpec::new(&id, granted);
            let egress = EgressPlan {
                network,
                ..spec.egress
            };
            match inner.prepare(spec.with_egress(egress)).await {
                Ok(sandbox) => Ok(Slot { sandbox, kind }),
                Err(error) => Err((Some(id), error)),
            }
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
        .notify(|(id, error): &Failed, delay: Duration| {
            let Logged { error_code, reason } = error.logged();
            let sandbox_id = id.as_ref().map(Uuid7::as_str);
            let retry_ms = saturating_millis(delay);
            let event = EVENT_SLOT_FAILED;
            tracing::warn!(sandbox_id, error_code, reason, retry_ms, event);
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
struct UntilClosed(mpsc::Sender<Slot>);

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

/// Destroys a slot no run will use.
pub(super) async fn retire(Slot { sandbox, .. }: Slot) {
    let id = sandbox.id().clone();
    if let Err(error) = sandbox.destroy().await {
        let Logged { error_code, reason } = error.logged();
        let sandbox_id = id.as_str();
        let event = EVENT_SLOT_LEFT;
        {
            let message = format_args!("an unused warm slot failed to tear down");
            tracing::warn!(message, error_code, sandbox_id, reason, event);
        };
    }
}
