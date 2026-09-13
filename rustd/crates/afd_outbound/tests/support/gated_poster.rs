//! A poster whose destinations answer when the test says.
//!
//! Shared by the lane suite: the delivery-side fairness dimension needs a
//! destination that will not answer, played by a gate the test holds shut.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use afd_datastore::OutboundDelivery;
use afd_outbound::{Deliver, Verdict};
use dashmap::DashMap;
use tokio::sync::Notify;

/// How long one ungated delivery holds its permit, so the ceiling bites.
pub(crate) const HOLD: Duration = Duration::from_millis(30);

/// A poster whose deliveries the test can hold and count.
///
/// Every delivery to a gated workspace waits on that workspace's gate; every
/// delivery to any other workspace answers after [`HOLD`]. Both record what
/// they delivered and the number of deliveries in flight when they started.
/// A handle over shared state, so the lanes own one and the test keeps one.
#[derive(Debug, Clone, Default)]
pub(crate) struct Gated {
    inner: Arc<GatedInner>,
}

#[derive(Debug, Default)]
struct GatedInner {
    gates: DashMap<String, Arc<Notify>>,
    /// An ordered log two tasks append to, which is what a lock is for; the
    /// gates beside it are keyed, so they go in the sharded map.
    delivered: Mutex<Vec<String>>,
    in_flight: AtomicUsize,
    high_water: AtomicUsize,
}

impl Gated {
    /// Holds every delivery to `workspace` until the returned gate is
    /// notified.
    pub(crate) fn shut(&self, workspace: &str) -> Arc<Notify> {
        let gate = Arc::new(Notify::new());
        self.inner
            .gates
            .insert(workspace.to_owned(), Arc::clone(&gate));
        gate
    }

    /// The `event_id`s delivered so far, in order.
    pub(crate) fn delivered(&self) -> Vec<String> {
        self.inner
            .delivered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many deliveries are in flight right now.
    pub(crate) fn in_flight(&self) -> usize {
        self.inner.in_flight.load(Ordering::Acquire)
    }

    /// The most deliveries ever in flight at once.
    pub(crate) fn high_water(&self) -> usize {
        self.inner.high_water.load(Ordering::Acquire)
    }
}

impl Deliver for Gated {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let gate = self
            .inner
            .gates
            .get(&job.workspace_id)
            .map(|gate| Arc::clone(&gate));
        let event_id = job.event_id.clone();
        let inner = Arc::clone(&self.inner);
        async move {
            let now = inner.in_flight.fetch_add(1, Ordering::AcqRel) + 1;
            inner.high_water.fetch_max(now, Ordering::AcqRel);
            match gate {
                Some(gate) => gate.notified().await,
                None => tokio::time::sleep(HOLD).await,
            }
            inner
                .delivered
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event_id);
            inner.in_flight.fetch_sub(1, Ordering::AcqRel);
            Verdict::Delivered
        }
    }
}
