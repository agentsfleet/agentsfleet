//! An executor whose processes are scripted, for the suites that prove the
//! sandbox-side handlers and a run's end without a sandbox.
//!
//! Each spawn runs the next script on fresh events and records what it was
//! asked. A script that stays open keeps its feed, so its process runs
//! until the suite ends it or a kill does; one that echoes answers each write
//! with the same bytes, as `cat` does. Like the real executor, it refuses a
//! write or a kill naming a process it does not hold.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use afr_executor::{
    Ending, Events, Executor, Feed, FileContent, Listing, Process, ProcessId, Spawn, Stream,
};
use bytes::Bytes;

/// The signal a kill ends a scripted process with.
const KILLED: i32 = 9;
/// What a spawn past the last script fails with.
const NO_SCRIPT: &str = "no scripted process left";
/// What a write or a kill naming a process it does not hold fails with.
const UNKNOWN_PROCESS: &str = "no process with that identifier";
/// What a file call fails with.
const PROCESSES_ONLY: &str = "a scripted executor runs processes only";

/// One scripted process: what it prints, and what happens after.
#[derive(Debug)]
pub struct ScriptedProcess {
    output: Option<Bytes>,
    then: Then,
}

/// What a scripted process does once it has printed.
#[derive(Debug)]
enum Then {
    /// Ends as this.
    Ends(Ending),
    /// Runs until the suite ends it or a kill does, printing back what is
    /// written to it when it echoes.
    StaysOpen { echo: bool },
    /// Its events finish with no ending, as a lost executor's do.
    Vanishes,
}

impl ScriptedProcess {
    /// Prints `output` and ends as `ending`.
    #[must_use]
    pub fn ends(output: &str, ending: Ending) -> Self {
        Self {
            output: said(output),
            then: Then::Ends(ending),
        }
    }

    /// Prints `output` and exits with `code`.
    #[must_use]
    pub fn exits(output: &str, code: i32) -> Self {
        Self::ends(output, Ending::Exited(code))
    }

    /// Prints `output` and stays open until the suite ends it or a kill does.
    #[must_use]
    pub fn stays_open(output: &str) -> Self {
        Self {
            output: said(output),
            then: Then::StaysOpen { echo: false },
        }
    }

    /// Stays open and prints back whatever is written to it.
    #[must_use]
    pub fn echoes() -> Self {
        Self {
            output: None,
            then: Then::StaysOpen { echo: true },
        }
    }

    /// Prints `output`, then its events finish with no ending, as a lost
    /// executor's do.
    #[must_use]
    pub fn vanishes(output: &str) -> Self {
        Self {
            output: said(output),
            then: Then::Vanishes,
        }
    }
}

/// `output` as one chunk; nothing for no output.
fn said(output: &str) -> Option<Bytes> {
    (!output.is_empty()).then(|| Bytes::copy_from_slice(output.as_bytes()))
}

/// A process the executor holds: where its events go, and whether it echoes.
#[derive(Debug)]
struct Held {
    id: ProcessId,
    feed: Feed,
    echo: bool,
}

/// An executor that runs scripted processes and records what it was asked.
#[derive(Debug, Default)]
pub struct ScriptedExecutor {
    scripts: Mutex<VecDeque<ScriptedProcess>>,
    ids: AtomicU64,
    spawned: Mutex<Vec<Spawn>>,
    written: Mutex<Vec<(ProcessId, Bytes)>>,
    killed: Mutex<Vec<ProcessId>>,
    held: Mutex<Vec<Held>>,
    /// Processes it no longer holds whose events stay open, as when an
    /// executor lost a process before its ending reached the caller.
    forgotten: Mutex<Vec<Held>>,
}

impl ScriptedExecutor {
    /// An executor whose spawns run `scripts`, in order.
    #[must_use]
    pub fn new(scripts: impl IntoIterator<Item = ScriptedProcess>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into_iter().collect()),
            ..Self::default()
        }
    }

    /// Every spawn it was asked for, in order.
    #[must_use]
    pub fn spawned(&self) -> Vec<Spawn> {
        locked(&self.spawned).clone()
    }

    /// Every write it was asked for, in order.
    #[must_use]
    pub fn written(&self) -> Vec<(ProcessId, Bytes)> {
        locked(&self.written).clone()
    }

    /// Every kill it was asked for, in order.
    #[must_use]
    pub fn killed(&self) -> Vec<ProcessId> {
        locked(&self.killed).clone()
    }

    /// Ends the held process `id` as `ending`; whether it held one.
    pub fn end(&self, id: ProcessId, ending: Ending) -> bool {
        let Some(held) = self.release(id) else {
            return false;
        };
        held.feed.end(ending);
        true
    }

    /// Stops holding process `id` while its events stay open; whether it
    /// held one.
    pub fn forget(&self, id: ProcessId) -> bool {
        let Some(held) = self.release(id) else {
            return false;
        };
        locked(&self.forgotten).push(held);
        true
    }

    fn release(&self, id: ProcessId) -> Option<Held> {
        let mut held = locked(&self.held);
        let at = held.iter().position(|open| open.id == id)?;
        Some(held.swap_remove(at))
    }
}

/// `mutex`'s guard. A suite that panicked holding it has already failed, and
/// what it recorded is still worth reading.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The executor's refusal, saying `detail`.
fn refused(detail: &str) -> afr_executor::Error {
    std::io::Error::other(detail).into()
}

#[async_trait::async_trait]
impl Executor for ScriptedExecutor {
    async fn spawn(&self, spawn: &Spawn) -> afr_executor::Result<Process> {
        locked(&self.spawned).push(spawn.clone());
        let script = locked(&self.scripts)
            .pop_front()
            .ok_or_else(|| refused(NO_SCRIPT))?;
        let id = ProcessId::new(self.ids.fetch_add(1, Ordering::SeqCst) + 1);
        let (feed, events) = Events::channel();
        if let Some(data) = script.output {
            feed.output(Stream::Stdout, data);
        }
        match script.then {
            Then::Ends(ending) => feed.end(ending),
            Then::StaysOpen { echo } => locked(&self.held).push(Held { id, feed, echo }),
            Then::Vanishes => drop(feed),
        }
        Ok(Process { id, events })
    }

    async fn write(&self, process: ProcessId, data: Bytes) -> afr_executor::Result<()> {
        locked(&self.written).push((process, data.clone()));
        let held = locked(&self.held);
        let open = held
            .iter()
            .find(|open| open.id == process)
            .ok_or_else(|| refused(UNKNOWN_PROCESS))?;
        if open.echo {
            open.feed.output(Stream::Stdout, data);
        }
        Ok(())
    }

    async fn kill(&self, process: ProcessId) -> afr_executor::Result<()> {
        locked(&self.killed).push(process);
        self.end(process, Ending::Signaled(KILLED))
            .then_some(())
            .ok_or_else(|| refused(UNKNOWN_PROCESS))
    }

    async fn read_file(&self, _path: &str, _max_bytes: u64) -> afr_executor::Result<FileContent> {
        Err(refused(PROCESSES_ONLY))
    }

    async fn write_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        Err(refused(PROCESSES_ONLY))
    }

    async fn append_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        Err(refused(PROCESSES_ONLY))
    }

    async fn delete_file(&self, _path: &str) -> afr_executor::Result<()> {
        Err(refused(PROCESSES_ONLY))
    }

    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        Err(refused(PROCESSES_ONLY))
    }
}

#[cfg(test)]
#[path = "scripted/tests.rs"]
mod tests;
