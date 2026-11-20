//! Run registrations preserve cancellation across worker scheduling and reconnects.
//!
//! A cancellation the daemon decided — a person's Stop, read from the run's
//! durable mark — is remembered beside the token it trips, so the run reports
//! `stopped` rather than a failure. Every other cancellation (a refused
//! session, a halt) trips the token alone.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};

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
    /// The registered runs the daemon stopped; each leaves with its registration.
    stopped: HashSet<Uuid7>,
    capacity: usize,
    offers_seen: u64,
    next_ready: NonZeroU64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runs: HashMap::new(),
            stopped: HashSet::new(),
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

    /// Trips `run`'s token, remembering it as stopped when the daemon said a
    /// person stopped it. False when no such run is registered.
    pub(crate) fn cancel(&self, run: &Uuid7, stopped: bool) -> bool {
        let stop = {
            let mut state = self.state.lock();
            let stop = state.runs.get(run).cloned();
            if stop.is_some() && stopped {
                state.stopped.insert(run.clone());
            }
            stop
        };
        stop.inspect(CancellationToken::cancel).is_some()
    }

    /// Whether the daemon stopped `run` while it was registered.
    pub(crate) fn stopped(&self, run: &Uuid7) -> bool {
        self.state.lock().stopped.contains(run)
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
        let mut state = self.running.state.lock();
        state.runs.remove(&self.run);
        state.stopped.remove(&self.run);
        drop(state);
        self.running.changed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "test target: a fixture that cannot be built is a broken test"
    )]

    use std::sync::Arc;

    use af_core::id::Uuid7;

    use super::Running;
    use crate::test_support::RUN_ID;

    #[test]
    fn only_a_daemon_stop_marks_a_run_stopped() {
        let running = Arc::new(Running::default());
        let run = Uuid7::parse(RUN_ID).unwrap();
        let registration = running.offer(run.clone(), false).unwrap();

        assert!(running.cancel(&run, false));
        assert!(registration.stop.is_cancelled());
        assert!(!running.stopped(&run), "a plain cancel is not a Stop");

        assert!(running.cancel(&run, true));
        assert!(running.stopped(&run));

        drop(registration);
        assert!(
            !running.stopped(&run),
            "the mark leaves with its registration"
        );
        assert!(!running.cancel(&run, true), "no run is registered any more");
        assert!(
            !running.stopped(&run),
            "an unregistered run is never marked"
        );
    }

    #[test]
    fn cancel_all_trips_every_run_and_stops_none() {
        let running = Arc::new(Running::default());
        let run = Uuid7::parse(RUN_ID).unwrap();
        let registration = running.offer(run.clone(), false).unwrap();
        running.cancel_all();
        assert!(registration.stop.is_cancelled());
        assert!(!running.stopped(&run));
    }
}
