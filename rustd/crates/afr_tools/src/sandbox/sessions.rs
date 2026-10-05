//! The processes a lease keeps open across calls.
//!
//! One owner: a session's `Process` lives in this registry and nowhere else,
//! so the run's end closes each one exactly once. Calls run one after another
//! and the registry is lent to one call at a time through the lease, so it
//! needs no lock. A session's start and end are logged here, on every path a
//! session leaves by, so the pair is complete by construction.

use std::collections::BTreeMap;

use afr_executor::{Ending, Executor, Process, ProcessId};

use super::output::Collected;

/// The most sessions one lease keeps open: Codex's
/// `MAX_UNIFIED_EXEC_PROCESSES`.
pub(super) const SESSIONS_PER_LEASE_MAX: usize = 64;
/// The event a session opening logs under.
const EVENT_STARTED: &str = "exec_session_started";
/// The event a session leaving the registry logs under.
const EVENT_COMPLETED: &str = "exec_session_completed";
/// The event a kill the executor refused logs under.
const EVENT_KILL_REFUSED: &str = "exec_session_kill_refused";

/// The sessions one lease keeps open, by the id the executor gave each
/// process.
#[derive(Debug, Default)]
pub struct Sessions {
    open: BTreeMap<ProcessId, Process>,
}

impl Sessions {
    /// Whether another session may open under the per-lease cap.
    pub(super) fn has_room(&self) -> bool {
        self.open.len() < SESSIONS_PER_LEASE_MAX
    }

    /// Keeps `process` as a session and lends it back to read. The executor
    /// never names two processes alike, so nothing open is replaced.
    pub(super) fn open(&mut self, process: Process) -> &mut Process {
        let session_id = process.id.get();
        let event = EVENT_STARTED;
        tracing::debug!(session_id, event);
        self.open.entry(process.id).or_insert(process)
    }

    /// The session named `id`, when it is open.
    pub(super) fn get_mut(&mut self, id: ProcessId) -> Option<&mut Process> {
        self.open.get_mut(&id)
    }

    /// Forgets session `id`, whose process ended as `ending`.
    pub(super) fn close(&mut self, id: ProcessId, ending: Ending) {
        self.open.remove(&id);
        completed(id, ending);
    }

    /// Ends every open session and answers how many it killed.
    ///
    /// A session whose process already ended is forgotten; the rest are
    /// killed through `executor`. A kill the executor refuses is logged and
    /// the session forgotten all the same: the sandbox's end stops whatever
    /// a kill could not.
    pub async fn close_all(&mut self, executor: &dyn Executor) -> usize {
        let mut killed = 0;
        for (id, mut process) in std::mem::take(&mut self.open) {
            if let Some(ending) = Collected::default().arrived(&mut process) {
                completed(id, ending);
                continue;
            }
            if let Err(refused) = executor.kill(id).await {
                let session_id = id.get();
                let reason = refused.to_string();
                let event = EVENT_KILL_REFUSED;
                tracing::debug!(session_id, reason, event);
            }
            completed(id, Ending::Interrupted);
            killed += 1;
        }
        killed
    }
}

/// Logs session `id` leaving the registry, and how its process ended.
fn completed(id: ProcessId, ending: Ending) {
    let session_id = id.get();
    let ending = ending.kind();
    let event = EVENT_COMPLETED;
    tracing::debug!(session_id, ending, event);
}

#[cfg(test)]
#[path = "sessions/tests.rs"]
mod tests;
