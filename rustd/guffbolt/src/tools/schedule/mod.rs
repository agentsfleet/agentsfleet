//! The schedule tools, over the [`Scheduler`] each is built with: six cron
//! tools and `schedule`, a one-off at a moment.
//!
//! ```text
//!  cron_add, cron_list, cron_update ─┐
//!  cron_remove, cron_run, cron_runs ─┼─► Scheduler ─┬─► LocalScheduler: a file, read by `due`
//!  schedule (one moment) ────────────┘              └─► an embedder's service
//! ```
//!
//! A tool parses its arguments and asks its scheduler; the scheduler answers
//! in its own words, which the model reads as they came: a service's JSON,
//! or the local scheduler's. A scheduler that acts outside the process
//! derives its idempotency key from the call's [`CallRef`], so a retried call
//! adds nothing twice. Nothing here keeps a clock: the scheduler fires a job,
//! and the embedder runs what it fired.

pub mod cron;
mod local;
mod moment;
pub mod once;

use std::fmt;
use std::pin::Pin;

use crate::tools::Refusal;
use crate::transcript::CallRef;

pub use self::cron::{CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate};
pub use self::local::{Due, LocalScheduler};
pub use self::once::ScheduleOnce;

/// What a schedule call reads back when its scheduler answered with nothing,
/// such as a job removed outright.
const DONE: &str = "done";

/// One scheduler call's work: what the model reads back, or the refusal.
pub type ScheduleFuture<'a> = Pin<Box<dyn Future<Output = Result<String, Refusal>> + Send + 'a>>;

/// A job to add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Job<'a> {
    /// Five fields: minute, hour, day of month, month and day of week.
    pub cron: &'a str,
    /// The IANA zone the expression is read in; UTC when `None`.
    pub timezone: Option<&'a str>,
    /// What the run is asked to do each time it fires.
    pub message: &'a str,
    /// Whether it retires after its first fire.
    pub once: bool,
}

/// What a change to a job names; a field left `None` keeps its value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Change<'a> {
    /// A new expression.
    pub cron: Option<&'a str>,
    /// A new zone.
    pub timezone: Option<&'a str>,
    /// A new message.
    pub message: Option<&'a str>,
    /// Whether it stops firing (`true`) or fires again (`false`).
    pub paused: Option<bool>,
}

/// Which of a job's runs to read, newest first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Page<'a> {
    /// How many to read.
    pub limit: Option<u32>,
    /// The cursor the page before ended at.
    pub starting_after: Option<&'a str>,
}

/// Where a run's jobs are kept and fired.
///
/// Each method answers the text the model reads, in the scheduler's own
/// words, because what a job carries differs by scheduler: a service's sync
/// state, or who made it. An empty answer reads `done`. A method that changes
/// a job takes the call's [`CallRef`], which a scheduler keys a retry by.
pub trait Scheduler: Send + Sync + fmt::Debug {
    /// Adds `job`.
    fn add<'a>(&'a self, call: &'a CallRef, job: Job<'a>) -> ScheduleFuture<'a>;

    /// Every job.
    fn list(&self) -> ScheduleFuture<'_>;

    /// Changes the job named `id` as `change` says.
    fn update<'a>(
        &'a self,
        call: &'a CallRef,
        id: &'a str,
        change: Change<'a>,
    ) -> ScheduleFuture<'a>;

    /// Removes the job named `id`.
    fn remove<'a>(&'a self, call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a>;

    /// Fires the job named `id` now, as if its moment had come.
    fn run_now<'a>(&'a self, call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a>;

    /// The runs the job named `id` fired, a page at a time.
    fn runs<'a>(&'a self, id: &'a str, page: Page<'a>) -> ScheduleFuture<'a>;
}

/// The call's output: the scheduler's answer, `done` for an empty one, or
/// its refusal.
fn answered(reply: Result<String, Refusal>) -> crate::ToolOutput {
    match reply {
        Ok(text) if text.is_empty() => crate::ToolOutput::succeeded(DONE),
        Ok(text) => crate::ToolOutput::succeeded(text),
        Err(refusal) => refusal.into(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod local_tests;
