//! Run registrations preserve cancellation across worker scheduling and reconnects.
//!
//! A cancellation the daemon decided — a person's Stop, read from the run's
//! durable mark — is remembered beside the token it trips, so the run reports
//! `stopped` rather than a failure. Every other cancellation (a refused
//! session, a halt) trips the token alone.
//!
//! Credits never pass the engine's memory bound: a host whose memory admits
//! fewer sandboxes than it has workers offers only as many credits as it
//! admits.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};

use af_core::id::Uuid7;
use af_wire::protocol::{RUNS_MAX, Up};
use afr_engine::MemoryBound;
use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{Result, error::ErrorKind};

/// The event a capacity the memory bound holds below the worker count is
/// logged under.
const EVENT_CAPACITY_BOUNDED: &str = "runner_capacity_bounded";

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
    assignments_seen: u64,
    next_ready: NonZeroU64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runs: HashMap::new(),
            stopped: HashSet::new(),
            capacity: 0,
            assignments_seen: 0,
            next_ready: NonZeroU64::MIN,
        }
    }
}

impl Running {
    /// Offers `workers` credits, or fewer: no more than the frame carries,
    /// and no more than `bound` admits sandboxes. A bound below the worker
    /// count is logged each time it changes what is offered.
    pub(crate) fn capacity(&self, workers: usize, bound: Option<MemoryBound>) {
        let admitted = bound.map_or(usize::MAX, |bound| {
            usize::try_from(bound.sandboxes_max).unwrap_or(usize::MAX)
        });
        let capacity = workers.min(admitted).min(RUNS_MAX);
        let mut state = self.state.lock();
        if state.capacity != capacity {
            state.capacity = capacity;
            drop(state);
            if let Some(bound) = bound.filter(|_| admitted < workers) {
                bounded(workers, bound);
            }
            self.changed.notify_one();
        }
    }

    pub(crate) fn new_session(&self) {
        let mut state = self.state.lock();
        state.assignments_seen = 0;
        state.next_ready = NonZeroU64::MIN;
    }

    pub(crate) fn ready(&self) -> Result<Up<'static>> {
        let mut state = self.state.lock();
        let revision = state.next_ready;
        state.next_ready = revision.checked_add(1).ok_or(ErrorKind::StreamProtocol)?;
        Ok(Up::Ready {
            credits: state.capacity.saturating_sub(state.runs.len()),
            revision,
            observed_assignments: state.assignments_seen,
        })
    }

    pub(crate) fn snapshot(&self) -> Vec<Uuid7> {
        self.state.lock().runs.keys().cloned().collect()
    }

    pub(crate) fn assign(self: &Arc<Self>, run: Uuid7, cancelled: bool) -> Result<RunRegistration> {
        let mut state = self.state.lock();
        if state.runs.len() >= RUNS_MAX || state.runs.contains_key(&run) {
            return Err(ErrorKind::StreamProtocol.into());
        }
        state.assignments_seen = state
            .assignments_seen
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

/// Logs the host's memory bound holding its credits below `worker_count`.
fn bounded(worker_count: usize, bound: MemoryBound) {
    let MemoryBound {
        sandboxes_max,
        mem_available_bytes,
        memory_overcommit,
    } = bound;
    let event = EVENT_CAPACITY_BOUNDED;
    tracing::info!(
        worker_count,
        sandboxes_max,
        mem_available_bytes,
        memory_overcommit,
        event
    );
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

    use std::num::NonZeroU64;
    use std::sync::Arc;

    use af_core::id::Uuid7;
    use af_wire::protocol::Up;
    use afr_engine::MemoryBound;

    use super::Running;
    use crate::test_support::RUN_ID;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn credits(running: &Running) -> Option<usize> {
        match running.ready().unwrap() {
            Up::Ready { credits, .. } => Some(credits),
            _other => None,
        }
    }

    #[test]
    fn test_credits_stop_at_the_overcommitted_memory_bound() {
        let sandbox = NonZeroU64::new(2 * GIB + 16 * MIB).unwrap();
        let running = Running::default();

        let overcommitted = MemoryBound::overcommitted(28 * GIB, 4, sandbox);
        running.capacity(64, Some(overcommitted));
        assert_eq!(overcommitted.sandboxes_max, 55);
        assert_eq!(credits(&running), Some(55));

        let exact = MemoryBound::overcommitted(28 * GIB, 1, sandbox);
        running.capacity(64, Some(exact));
        assert_eq!(exact.sandboxes_max, 13);
        assert_eq!(credits(&running), Some(13));

        running.capacity(8, Some(overcommitted));
        assert_eq!(credits(&running), Some(8), "fewer workers than the bound");
        running.capacity(64, None);
        assert_eq!(credits(&running), Some(64), "no bound, the workers");
    }

    #[test]
    fn only_a_daemon_stop_marks_a_run_stopped() {
        let running = Arc::new(Running::default());
        let run = Uuid7::parse(RUN_ID).unwrap();
        let registration = running.assign(run.clone(), false).unwrap();

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
        let registration = running.assign(run.clone(), false).unwrap();
        running.cancel_all();
        assert!(registration.stop.is_cancelled());
        assert!(!running.stopped(&run));
    }
}
