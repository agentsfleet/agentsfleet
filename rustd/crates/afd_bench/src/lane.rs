//! The measurement lanes themselves.
//!
//! Each lane is a driver over the production types plus a reporter, split so
//! the thing that generates load and the thing that writes the file can be read
//! and changed apart.

pub mod cardinality;
pub mod lease;
pub mod outbound;
pub mod outcomes;
pub mod steer;
pub mod sweep;
pub mod tail;

use tokio::task::JoinError;

use crate::error::{ErrorKind, Result};

/// A spawned task's outcome, with a lost task named by what it was doing.
///
/// A join fails only when the task panicked or was cancelled, and either way
/// the window measured less than it drove; the role says which task a reader
/// should look for in the panic the runtime already printed.
///
/// # Errors
///
/// `TaskLost` naming `role`.
pub(crate) fn joined<T>(
    outcome: core::result::Result<T, JoinError>,
    role: &'static str,
) -> Result<T> {
    outcome.map_err(|_lost| ErrorKind::TaskLost { role }.into())
}

#[cfg(test)]
mod tests;
