//! The executor a run's sandbox-side calls use, handed over once by the
//! lease's build.
//!
//! ```text
//!   build ─► prepare ─► checkout ─► fill(Ok(executor)) ─┐
//!   run   ─► model call ─► sandbox call ─► get() waits ─┴─► runs in that sandbox
//!                          supervisor call ─► never waits
//! ```
//!
//! The run starts before its sandbox is ready, so a call that needs the
//! sandbox waits here for it, and a call that does not never touches the slot.
//! The slot holds a borrow: the lease owns the sandbox, and outlives both the
//! build that fills the slot and the run that reads it. A build that fails
//! fills the slot with why, so every waiter, and every call after, answers at
//! once instead of waiting for a sandbox that will never come.

use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use crate::Executor;

/// What a filled slot holds: the executor, or why there is none.
pub type Outcome<'a> = Result<&'a dyn Executor, Unavailable>;

/// Why a run has no executor to lend a sandbox-side call. Each reads as the
/// end of a sentence that starts with the tool's name, and names no host,
/// address or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// The lease runs no sandbox: every tool it offers runs in the supervisor.
    NoSandbox,
    /// The sandbox could not be built.
    Prepare,
    /// A bound repository would not check out into the sandbox.
    Checkout,
    /// The lease ended before its sandbox was ready.
    Stopped,
}

impl Unavailable {
    /// The class, as a log field names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSandbox => "no_sandbox",
            Self::Prepare => "prepare",
            Self::Checkout => "checkout",
            Self::Stopped => "stopped",
        }
    }

    /// What a sandbox-side call reads back, after the tool's name.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoSandbox => "runs in the sandbox, and this run has none",
            Self::Prepare => "runs in the sandbox, and this run's sandbox could not be prepared",
            Self::Checkout => {
                "runs in the sandbox, and this run's repositories could not be checked out into it"
            }
            Self::Stopped => {
                "runs in the sandbox, and the lease ended before its sandbox was ready"
            }
        }
    }
}

/// The slot's state, behind one lock so a fill and a waiter never miss each
/// other.
#[derive(Debug, Default)]
struct State<'a> {
    outcome: Option<Outcome<'a>>,
    /// When the first call found the slot empty.
    first_wait: Option<Instant>,
    /// From that call to the fill: how long sandbox calls were held up.
    held: Option<Duration>,
}

/// A run's executor, filled once by the lease's build and read by every
/// sandbox-side call.
#[derive(Debug, Default)]
pub struct ExecutorSlot<'a> {
    state: Mutex<State<'a>>,
    filled: Notify,
}

impl<'a> ExecutorSlot<'a> {
    /// A slot the build has yet to fill.
    #[must_use]
    pub fn pending() -> Self {
        Self::default()
    }

    /// A slot holding `executor` from the start: what a caller passes when it
    /// has no build to wait on, such as a suite driving a run against an
    /// executor it already started.
    #[must_use]
    pub fn ready(executor: &'a dyn Executor) -> Self {
        Self::filled_with(Ok(executor))
    }

    /// A slot that will never hold an executor, for `why`.
    #[must_use]
    pub fn unavailable(why: Unavailable) -> Self {
        Self::filled_with(Err(why))
    }

    fn filled_with(outcome: Outcome<'a>) -> Self {
        let slot = Self::pending();
        slot.lock().outcome = Some(outcome);
        slot
    }

    /// Fills the slot with `outcome` and wakes every call waiting on it.
    /// Answers whether this call filled it: the first fill stands, and a
    /// later one changes nothing.
    pub fn fill(&self, outcome: Outcome<'a>) -> bool {
        let mut state = self.lock();
        if state.outcome.is_some() {
            return false;
        }
        state.outcome = Some(outcome);
        state.held = state.first_wait.map(|first| first.elapsed());
        drop(state);
        self.filled.notify_waiters();
        true
    }

    /// How long sandbox calls were held up: from the first call that found
    /// the slot empty to its fill. None when no call ever waited.
    #[must_use]
    pub fn held(&self) -> Option<Duration> {
        self.lock().held
    }

    /// The outcome when the slot is filled; otherwise notes that a call is
    /// waiting, so the fill can say how long. The lock never spans an await.
    fn outcome_or_wait(&self) -> Option<Outcome<'a>> {
        let mut state = self.lock();
        if state.outcome.is_none() {
            state.first_wait.get_or_insert_with(Instant::now);
        }
        state.outcome
    }

    fn lock(&self) -> MutexGuard<'_, State<'a>> {
        // Nothing panics while the state is written, so the lock is never
        // poisoned; a poisoned one still holds a whole state.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Where a run's sandbox-side calls get their executor.
///
/// The run holds this, never the slot itself: the slot's borrow of the
/// sandbox is its own, so the run's lifetimes stay free of it.
#[async_trait::async_trait]
pub trait ExecutorSource: Send + Sync + fmt::Debug {
    /// The executor, or why there is none, waiting for the build when it has
    /// not finished.
    ///
    /// # Errors
    /// [`Unavailable`]: the run has no executor, and why.
    async fn get(&self) -> Outcome<'_>;

    /// The executor when there is one already, without waiting: for run end
    /// and session close, which use only a sandbox that is already there.
    fn peek(&self) -> Option<&dyn Executor>;

    /// Whether a call reading the source now will not wait.
    fn is_filled(&self) -> bool;
}

#[async_trait::async_trait]
impl ExecutorSource for ExecutorSlot<'_> {
    async fn get(&self) -> Outcome<'_> {
        loop {
            // Made before the state is read: a fill between the read and the
            // wait still wakes it.
            let filled = self.filled.notified();
            if let Some(outcome) = self.outcome_or_wait() {
                return outcome;
            }
            filled.await;
        }
    }

    fn peek(&self) -> Option<&dyn Executor> {
        self.lock().outcome.and_then(Result::ok)
    }

    fn is_filled(&self) -> bool {
        self.lock().outcome.is_some()
    }
}

#[cfg(test)]
mod tests;
