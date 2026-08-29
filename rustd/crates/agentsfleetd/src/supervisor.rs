//! Every long-lived task, and the promise that none outlives what it reads.
//!
//! # Invariant C2, and why it needs a type
//!
//! Stop, then join, THEN drop. Nothing shared may be freed while a task that
//! touches it can still run — a pool closed while a sweeper still holds a
//! connection is a use-after-free in a language that was supposed to have
//! made that unwritable, because the unsafety moved from memory into
//! lifetimes of tasks.
//!
//! Rust does not enforce it for `tokio::spawn`: a spawned task is detached and
//! outlives its spawner by default, and nothing warns you. So the ordering
//! becomes a type — [`Supervisor`] owns every handle, and the only way to get
//! at what a task borrowed is to go through a shutdown that joined it.
//!
//! # Two flags in Zig, a sequence here
//!
//! `serve_shutdown.zig` keeps `shutdown_requested` and `background_stop` apart
//! so a signal arriving DURING boot cannot kill the background stack while the
//! server may still come up and briefly serve. It needs two flags because its
//! watcher is a separate thread polling every 100ms, so "the server stopped"
//! and "the signal arrived" are events that race.
//!
//! Here they do not race, because they are statements in order: await the
//! server's own graceful shutdown, and only then cancel the supervisor. A
//! signal during boot leaves this token untouched, the server comes up, stops
//! immediately because its shutdown future has already resolved, and the
//! background stack is cancelled after — which is the property those two flags
//! were protecting. One fewer piece of shared mutable state, and the ordering
//! is readable in the function that performs it.
//!
//! # Why the token rather than a shared bool
//!
//! [`CancellationToken`] is edge-triggered: `cancelled().await` wakes the
//! instant it fires. The Zig watcher polls at 100ms and pays that latency on
//! every shutdown, on every task. It also composes — a task selecting over its
//! own I/O and `cancelled()` is interrupted mid-read, which is what Dimension
//! 7.5 asks to be PROVEN rather than assumed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// How long a task may take to notice cancellation before it is reported.
///
/// A deadline at the call site, per Invariant 4, and it exists because the
/// alternative is worse: a task that ignores its token would hang shutdown
/// forever, and an operator would see a process that will not die rather than
/// a line naming the task that would not stop.
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(10);

/// The supervised-task boundary's three stable event names.
///
/// One spelling each, because a log pipeline selects on these strings and a
/// second spelling of a terminal event is a silently missing alert. `FAILED`
/// covers both terminal failures — a panic and a join timeout — which is why
/// it appears twice below and why the `reason` field, not the event name,
/// distinguishes them.
const EVENT_STARTED: &str = "supervised_task_started";
const EVENT_COMPLETED: &str = "supervised_task_completed";
const EVENT_FAILED: &str = "supervised_task_failed";

/// Correlates task lifecycle events across every supervisor in the process.
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(0);

/// One supervised task.
#[derive(Debug)]
struct Supervised {
    /// Correlates this task's started record with exactly one terminal record.
    task_id: u64,
    /// What it is, for the line that names it if it will not stop.
    name: &'static str,
    /// When its started event was emitted, for every terminal event's timing.
    started: Instant,
    handle: JoinHandle<()>,
}

/// What a shutdown did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Tasks that stopped and were joined, in the order they were spawned.
    pub joined: Vec<&'static str>,
    /// Tasks still running when [`JOIN_TIMEOUT`] expired.
    ///
    /// Non-empty means a task is not selecting over its cancellation token —
    /// a bug in that task, and the report names it rather than leaving an
    /// operator with a process that will not exit.
    pub abandoned: Vec<&'static str>,
    /// Tasks that ended by panicking.
    pub panicked: Vec<&'static str>,
}

impl ShutdownReport {
    /// Whether every task stopped when it was asked to.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.abandoned.is_empty() && self.panicked.is_empty()
    }
}

/// Owns every long-lived task and the token they stop on.
#[derive(Debug)]
pub struct Supervisor {
    token: CancellationToken,
    tasks: Vec<Supervised>,
}

impl Supervisor {
    /// A supervisor with nothing running.
    #[must_use]
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            tasks: Vec::new(),
        }
    }

    /// The token supervised tasks stop on.
    ///
    /// Handed out so a task that owns I/O can select over it directly. A task
    /// that only polls it between iterations is not cancellable mid-read,
    /// which is the failure Dimension 7.5 exists to catch.
    #[must_use]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Spawns `task` under this supervisor, handing it a cancellation token.
    ///
    /// The token is a parameter rather than something the task reaches for,
    /// so a task that forgot to take one does not compile into the inventory.
    pub fn spawn<F>(&mut self, name: &'static str, task: impl FnOnce(CancellationToken) -> F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let started = Instant::now();
        let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            task_id,
            task = name,
            event = EVENT_STARTED,
            "supervised task started"
        );
        self.tasks.push(Supervised {
            task_id,
            name,
            started,
            handle: tokio::spawn(task(self.token.clone())),
        });
    }

    /// Every task currently supervised, in the order they were spawned.
    ///
    /// The inventory Dimension 7.5 asks to be complete. Exposed so the check
    /// is a test over the running process rather than a list in a document
    /// that drifts from it.
    #[must_use]
    pub fn inventory(&self) -> Vec<&'static str> {
        self.tasks.iter().map(|task| task.name).collect()
    }

    /// Cancels every task and waits for each to stop.
    ///
    /// Consumes the supervisor, which is the point: whatever these tasks
    /// borrowed cannot be dropped until this returns, because the caller has
    /// to sequence it after.
    pub async fn shutdown(self) -> ShutdownReport {
        let count = self.tasks.len();
        tracing::info!(
            tasks = count,
            event = "supervised_tasks_cancelling",
            "cancelling supervised tasks"
        );
        self.token.cancel();

        let mut report = ShutdownReport::default();
        for task in self.tasks {
            settle(task, &mut report).await;
        }
        report
    }
}

async fn settle(task: Supervised, report: &mut ShutdownReport) {
    let Supervised {
        task_id,
        name,
        started,
        handle,
    } = task;
    match tokio::time::timeout(JOIN_TIMEOUT, handle).await {
        Ok(Ok(())) => completed(task_id, name, started, report),
        Ok(Err(_panicked)) => panicked(task_id, name, started, report),
        Err(_elapsed) => timed_out(task_id, name, started, report),
    }
}

fn completed(task_id: u64, name: &'static str, started: Instant, report: &mut ShutdownReport) {
    let duration_ms = started.elapsed().as_millis();
    tracing::info!(
        task_id,
        task = name,
        duration_ms,
        event = EVENT_COMPLETED,
        "supervised task completed"
    );
    report.joined.push(name);
}

fn panicked(task_id: u64, name: &'static str, started: Instant, report: &mut ShutdownReport) {
    let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
    let duration_ms = started.elapsed().as_millis();
    tracing::error!(
        error_code,
        task_id,
        task = name,
        duration_ms,
        reason = "panicked",
        event = EVENT_FAILED,
        "supervised task panicked"
    );
    report.panicked.push(name);
}

fn timed_out(task_id: u64, name: &'static str, started: Instant, report: &mut ShutdownReport) {
    let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
    let timeout_ms = JOIN_TIMEOUT.as_millis();
    let duration_ms = started.elapsed().as_millis();
    tracing::error!(
        error_code,
        task_id,
        task = name,
        timeout_ms,
        duration_ms,
        reason = "join_timeout",
        event = EVENT_FAILED,
        "supervised task did not stop when cancelled — it is not selecting over its cancellation token"
    );
    report.abandoned.push(name);
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
