//! The processes a lease keeps open across calls.
//!
//! One owner: a session's `Process` lives in this registry and nowhere else,
//! so the run's end closes each one exactly once. A run's child loops call
//! while their parent does, so the registry is shared: the book of open
//! sessions sits behind a lock held only to look up, add or remove, and each
//! process behind its own, held by the one call reading it across its yield.
//! Two calls on one session take turns; calls on two sessions never wait on
//! each other. A session's start and end are logged here, on every path a
//! session leaves by, and only by the path that removed it, so the pair is
//! complete and single by construction.
//!
//! At the cap a new session makes room as Codex's `prune_processes_if_needed`
//! does: the least recently used session that already ended goes, else the
//! least recently used one still running is killed, and the most recently
//! used few are never chosen. A session a call is reading is never chosen
//! either.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use afr_executor::{Ending, Executor, Process, ProcessId};
use tokio::sync::Mutex as Held;

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

/// A session's process, held by the one call reading it.
pub(super) type Shared = Arc<Held<Process>>;

/// One open session: its process, and when a call last used it.
#[derive(Debug)]
struct Session {
    process: Shared,
    used: u64,
}

/// The open sessions, by the id the executor gave each process.
#[derive(Debug, Default)]
struct Book {
    open: BTreeMap<ProcessId, Session>,
    /// Counts every open and every lookup, so a larger `used` is more recent.
    uses: u64,
}

/// The sessions one lease keeps open.
#[derive(Debug, Default)]
pub struct Sessions {
    book: Mutex<Book>,
}

impl Sessions {
    /// Makes room for one more session when the lease already keeps the cap
    /// open, ending one as the module documents.
    pub(super) async fn make_room(&self, executor: &dyn Executor) {
        let pruned = {
            let mut book = self.book();
            if book.open.len() < SESSIONS_PER_LEASE_MAX {
                return;
            }
            book.pruned()
        };
        if let Some((id, process)) = pruned {
            end(executor, id, process).await;
        }
    }

    /// Keeps `process` as a session and hands it back to read. The executor
    /// never names two processes alike, so nothing open is replaced.
    pub(super) fn open(&self, process: Process) -> Shared {
        let session_id = process.id.get();
        let event = EVENT_STARTED;
        tracing::debug!(session_id, event);
        let mut book = self.book();
        let used = book.used();
        let id = process.id;
        let shared = Arc::new(Held::new(process));
        let session = Session {
            process: Arc::clone(&shared),
            used,
        };
        book.open.entry(id).or_insert(session);
        shared
    }

    /// The session named `id`, when it is open; it counts as used now.
    pub(super) fn get(&self, id: ProcessId) -> Option<Shared> {
        let mut book = self.book();
        let used = book.used();
        book.open.get_mut(&id).map(|session| {
            session.used = used;
            Arc::clone(&session.process)
        })
    }

    /// Forgets session `id`, whose process ended as `ending`. A session room
    /// was already made from, or another call already closed, is not logged
    /// again.
    pub(super) fn close(&self, id: ProcessId, ending: Ending) {
        if self.book().open.remove(&id).is_some() {
            completed(id, ending);
        }
    }

    /// Ends every open session and answers how many were still running.
    ///
    /// A session whose process already ended is forgotten; the rest are
    /// killed through `executor`. A kill the executor refuses is logged and
    /// the session forgotten all the same: the sandbox's end stops whatever
    /// a kill could not.
    pub async fn close_all(&self, executor: &dyn Executor) -> usize {
        let open = std::mem::take(&mut self.book().open);
        let mut running = 0;
        for (id, session) in open {
            if end(executor, id, session.process).await {
                running += 1;
            }
        }
        running
    }

    /// The book, whatever a panicking holder left: every change to it is one
    /// map operation, so a panic leaves it whole.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Book {
    /// Takes out the session room is made from, if any may go. A session a
    /// call holds is in use, so it is never a candidate.
    fn pruned(&mut self) -> Option<(ProcessId, Shared)> {
        let mut by_use: Vec<(u64, ProcessId, bool)> = self
            .open
            .iter()
            .filter_map(|(id, session)| {
                let process = session.process.try_lock().ok()?;
                Some((session.used, *id, process.events.is_finished()))
            })
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

    /// The next use's number.
    fn used(&mut self) -> u64 {
        self.uses += 1;
        self.uses
    }
}

/// Ends session `id`: forgotten when its process already ended, killed
/// through `executor` otherwise; whether it was still running.
async fn end(executor: &dyn Executor, id: ProcessId, process: Shared) -> bool {
    let mut process = process.lock().await;
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
