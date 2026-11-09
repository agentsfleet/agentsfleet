//! Lease registrations preserve cancellation across worker scheduling and reconnects.

use std::{collections::HashMap, num::NonZeroU64, sync::Arc};

use af_core::id::Uuid7;
use af_wire::protocol::{LEASES_MAX, Up};
use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{Result, error::ErrorKind};

#[derive(Debug, Default)]
pub(crate) struct Running {
    state: Mutex<State>,
    pub(crate) changed: Notify,
}

/// Capacity and registrations share one snapshot and one insertion bound.
#[derive(Debug)]
struct State {
    leases: HashMap<Uuid7, CancellationToken>,
    capacity: usize,
    offers_seen: u64,
    next_ready: NonZeroU64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            leases: HashMap::new(),
            capacity: 0,
            offers_seen: 0,
            next_ready: NonZeroU64::MIN,
        }
    }
}

impl Running {
    pub(crate) fn capacity(&self, workers: usize) {
        let mut state = self.state.lock();
        let capacity = workers.min(LEASES_MAX);
        if state.capacity != capacity {
            state.capacity = capacity;
            drop(state);
            self.changed.notify_one();
        }
    }

    pub(crate) fn new_session(&self) {
        let mut state = self.state.lock();
        state.offers_seen = 0;
        state.next_ready = NonZeroU64::MIN;
    }

    pub(crate) fn ready(&self) -> Result<Up<'static>> {
        let mut state = self.state.lock();
        let revision = state.next_ready;
        state.next_ready = revision.checked_add(1).ok_or(ErrorKind::StreamProtocol)?;
        Ok(Up::Ready {
            credits: state.capacity.saturating_sub(state.leases.len()),
            revision,
            observed_offers: state.offers_seen,
        })
    }

    pub(crate) fn snapshot(&self) -> Vec<Uuid7> {
        self.state.lock().leases.keys().cloned().collect()
    }

    pub(crate) fn offer(
        self: &Arc<Self>,
        lease: Uuid7,
        cancelled: bool,
    ) -> Result<LeaseRegistration> {
        let mut state = self.state.lock();
        if state.leases.len() >= LEASES_MAX || state.leases.contains_key(&lease) {
            return Err(ErrorKind::StreamProtocol.into());
        }
        state.offers_seen = state
            .offers_seen
            .checked_add(1)
            .ok_or(ErrorKind::StreamProtocol)?;
        let stop = CancellationToken::new();
        if cancelled {
            stop.cancel();
        }
        state.leases.insert(lease.clone(), stop.clone());
        drop(state);
        self.changed.notify_one();
        Ok(LeaseRegistration {
            running: Arc::clone(self),
            lease,
            stop,
        })
    }

    pub(crate) fn cancel(&self, lease: &Uuid7) -> bool {
        let stop = self.state.lock().leases.get(lease).cloned();
        if let Some(stop) = stop {
            stop.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_all(&self) {
        let stops: Vec<_> = self.state.lock().leases.values().cloned().collect();
        for stop in stops {
            stop.cancel();
        }
    }
}

#[derive(Debug)]
pub(crate) struct LeaseRegistration {
    running: Arc<Running>,
    pub(crate) lease: Uuid7,
    pub(crate) stop: CancellationToken,
}

impl Drop for LeaseRegistration {
    fn drop(&mut self) {
        self.running.state.lock().leases.remove(&self.lease);
        self.running.changed.notify_one();
    }
}
