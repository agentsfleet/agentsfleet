//! Run registrations preserve cancellation across worker scheduling and reconnects.

use std::{collections::HashMap, num::NonZeroU64, sync::Arc};

use af_core::id::Uuid7;
use af_wire::protocol::{RUNS_MAX, Up};
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
    runs: HashMap<Uuid7, CancellationToken>,
    capacity: usize,
    offers_seen: u64,
    next_ready: NonZeroU64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runs: HashMap::new(),
            capacity: 0,
            offers_seen: 0,
            next_ready: NonZeroU64::MIN,
        }
    }
}

impl Running {
    pub(crate) fn capacity(&self, workers: usize) {
        let mut state = self.state.lock();
        let capacity = workers.min(RUNS_MAX);
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
            credits: state.capacity.saturating_sub(state.runs.len()),
            revision,
            observed_offers: state.offers_seen,
        })
    }

    pub(crate) fn snapshot(&self) -> Vec<Uuid7> {
        self.state.lock().runs.keys().cloned().collect()
    }

    pub(crate) fn offer(self: &Arc<Self>, run: Uuid7, cancelled: bool) -> Result<RunRegistration> {
        let mut state = self.state.lock();
        if state.runs.len() >= RUNS_MAX || state.runs.contains_key(&run) {
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
        state.runs.insert(run.clone(), stop.clone());
        drop(state);
        self.changed.notify_one();
        Ok(RunRegistration {
            running: Arc::clone(self),
            run,
            stop,
        })
    }

    pub(crate) fn cancel(&self, run: &Uuid7) -> bool {
        let stop = self.state.lock().runs.get(run).cloned();
        stop.inspect(CancellationToken::cancel).is_some()
    }

    pub(crate) fn cancel_all(&self) {
        let stops: Vec<_> = self.state.lock().runs.values().cloned().collect();
        for stop in stops {
            stop.cancel();
        }
    }
}

#[derive(Debug)]
pub(crate) struct RunRegistration {
    running: Arc<Running>,
    pub(crate) run: Uuid7,
    pub(crate) stop: CancellationToken,
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        self.running.state.lock().runs.remove(&self.run);
        self.running.changed.notify_one();
    }
}
