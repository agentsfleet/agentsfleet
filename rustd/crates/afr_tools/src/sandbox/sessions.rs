//! The processes a lease keeps open across calls.
//!
//! One owner: a session's `Process` lives in this registry and nowhere else,
//! so the run's end closes each one exactly once. Calls run one after another
//! and the registry is lent to one call at a time through the lease, so it
//! needs no lock. A session's start and end are logged here, on every path a
//! session leaves by, so the pair is complete by construction.
//!
//! At the cap a new session makes room as Codex's `prune_processes_if_needed`
//! does: the least recently used session that already ended goes, else the
//! least recently used one still running is killed, and the most recently
//! used few are never chosen.

use std::collections::BTreeMap;

use afr_executor::{Ending, Executor, Process, ProcessId};

use super::output::Collected;

/// The most sessions one lease keeps open: Codex's
/// `MAX_UNIFIED_EXEC_PROCESSES`.
pub(super) const SESSIONS_PER_LEASE_MAX: usize = 64;
/// How many of the most recently used sessions room is never made from:
/// Codex's eight.
pub(super) const SESSIONS_PROTECTED: usize = 8;
/// The event a session opening logs under.
const EVENT_STARTED: &str = "exec_session_started";
/// The event a session leaving the registry logs under.
const EVENT_COMPLETED: &str = "exec_session_completed";
/// The event a kill the executor refused logs under.
const EVENT_KILL_REFUSED: &str = "exec_session_kill_refused";

/// One open session: its process, and when a call last used it.
#[derive(Debug)]
struct Session {
    process: Process,
    used: u64,
}

/// The sessions one lease keeps open, by the id the executor gave each
/// process.
#[derive(Debug, Default)]
pub struct Sessions {
    open: BTreeMap<ProcessId, Session>,
    /// Counts every open and every lookup, so a larger `used` is more recent.
    uses: u64,
}

impl Sessions {
    /// Makes room for one more session when the lease already keeps the cap
    /// open, ending one as the module documents.
    pub(super) async fn make_room(&mut self, executor: &dyn Executor) {
        if self.open.len() < SESSIONS_PER_LEASE_MAX {
            return;
        }
        if let Some((id, process)) = self.pruned() {
            end(executor, id, process).await;
        }
    }

    /// Takes out the session room is made from, if any may go.
    fn pruned(&mut self) -> Option<(ProcessId, Process)> {
        let mut by_use: Vec<(u64, ProcessId, bool)> = self
            .open
            .iter()
            .map(|(id, session)| (session.used, *id, session.process.events.is_closed()))
            .collect();
        by_use.sort_unstable();
        let unprotected = by_use.len().saturating_sub(SESSIONS_PROTECTED);
        let candidates = by_use.get(..unprotected).unwrap_or_default();
        let (_used, id, _ended) = candidates
            .iter()
            .find(|(_used, _id, ended)| *ended)
            .or_else(|| candidates.first())?;
        self.open.remove(id).map(|session| (*id, session.process))
    }

    /// Keeps `process` as a session and lends it back to read. The executor
    /// never names two processes alike, so nothing open is replaced.
    pub(super) fn open(&mut self, process: Process) -> &mut Process {
        let session_id = process.id.get();
        let event = EVENT_STARTED;
        tracing::debug!(session_id, event);
        let used = self.used();
        &mut self
            .open
            .entry(process.id)
            .or_insert(Session { process, used })
            .process
    }

    /// The session named `id`, when it is open; it counts as used now.
    pub(super) fn get_mut(&mut self, id: ProcessId) -> Option<&mut Process> {
        let used = self.used();
        self.open.get_mut(&id).map(|session| {
            session.used = used;
            &mut session.process
        })
    }

    /// The next use's number.
    fn used(&mut self) -> u64 {
        self.uses += 1;
        self.uses
    }

    /// Forgets session `id`, whose process ended as `ending`.
    pub(super) fn close(&mut self, id: ProcessId, ending: Ending) {
        self.open.remove(&id);
        completed(id, ending);
    }

    /// Ends every open session and answers how many were still running.
    ///
    /// A session whose process already ended is forgotten; the rest are
    /// killed through `executor`. A kill the executor refuses is logged and
    /// the session forgotten all the same: the sandbox's end stops whatever
    /// a kill could not.
    pub async fn close_all(&mut self, executor: &dyn Executor) -> usize {
        let mut running = 0;
        for (id, session) in std::mem::take(&mut self.open) {
            if end(executor, id, session.process).await {
                running += 1;
            }
        }
        running
    }
}

/// Ends session `id`: forgotten when its process already ended, killed
/// through `executor` otherwise; whether it was still running.
async fn end(executor: &dyn Executor, id: ProcessId, mut process: Process) -> bool {
    if let Some(ending) = Collected::default().arrived(&mut process) {
        completed(id, ending);
        return false;
    }
    if let Err(refused) = executor.kill(id).await {
        let session_id = id.get();
        let reason = refused.to_string();
        let event = EVENT_KILL_REFUSED;
        tracing::debug!(session_id, reason, event);
    }
    completed(id, Ending::Interrupted);
    true
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
