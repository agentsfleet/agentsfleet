//! Ending a window: every task started is joined before any failure is answered.
//!
//! A task left running when the window errors keeps claiming, renewing and
//! reading while the caller releases the platform rows and sweeps the fixtures
//! beneath it. So the first failure is held until the last task has ended.

use tokio::task::JoinHandle;

use crate::error::Result;
use crate::lane::joined;

/// Join every task, answering the first failure only once all have ended.
///
/// # Errors
///
/// The first task's own error, or `TaskLost` naming `role` for a task that
/// panicked or was cancelled. Later failures are dropped, not reordered ahead.
pub(super) async fn all<T>(
    tasks: Vec<JoinHandle<Result<T>>>,
    role: &'static str,
) -> Result<Vec<T>> {
    let mut done = Vec::with_capacity(tasks.len());
    let mut first = None;
    for task in tasks {
        match joined(task.await, role).and_then(|inner| inner) {
            Ok(value) => done.push(value),
            Err(failure) => first = first.or(Some(failure)),
        }
    }
    first.map_or(Ok(done), Err)
}

#[cfg(test)]
mod tests;
