//! A poster that records what it was handed, per lane, and how many it held.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use af_core::id::Uuid7;
use afd_outbound::{Claimed, Deliver, Verdict};

#[derive(Debug, Default)]
struct Log {
    attempted: Vec<Uuid7>,
    delivered: HashMap<Uuid7, Vec<Uuid7>>,
    in_flight: HashMap<Uuid7, usize>,
    most: HashMap<Uuid7, usize>,
}

/// Records every attempt; refuses one event retryably, if told to.
#[derive(Debug, Clone, Default)]
pub(crate) struct Recorder {
    log: Arc<Mutex<Log>>,
    failing: Option<Uuid7>,
}

impl Recorder {
    pub(crate) fn failing(event: Uuid7) -> Self {
        Self {
            failing: Some(event),
            ..Self::default()
        }
    }

    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn delivered_to(&self, workspace: &Uuid7) -> Vec<Uuid7> {
        self.log()
            .delivered
            .get(workspace)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn most_in_flight(&self, workspace: &Uuid7) -> usize {
        self.log().most.get(workspace).copied().unwrap_or_default()
    }

    pub(crate) fn attempted(&self) -> Vec<Uuid7> {
        self.log().attempted.clone()
    }
}

/// Records an attempt starting, and the most this lane has held at once.
fn started(log: &Mutex<Log>, workspace: &Uuid7, event: &Uuid7) {
    let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
    log.attempted.push(event.clone());
    let held = log.in_flight.entry(workspace.clone()).or_default();
    *held += 1;
    let now = *held;
    let most = log.most.entry(workspace.clone()).or_default();
    *most = (*most).max(now);
}

impl Deliver for Recorder {
    fn deliver(&self, job: &Claimed) -> impl Future<Output = Verdict> + Send {
        let (workspace, event) = (job.workspace_id.clone(), job.event_id.clone());
        let failing = self.failing.as_ref() == Some(&event);
        let log = Arc::clone(&self.log);
        async move {
            started(&log, &workspace, &event);
            // Long enough that two workers would overlap on one lane if the
            // claim let them.
            tokio::time::sleep(Duration::from_millis(5)).await;
            let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(held) = log.in_flight.get_mut(&workspace) {
                *held -= 1;
            }
            if failing {
                return Verdict::Retryable;
            }
            log.delivered.entry(workspace).or_default().push(event);
            Verdict::Delivered
        }
    }
}
