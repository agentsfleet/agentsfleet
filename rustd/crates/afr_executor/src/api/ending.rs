//! How a process ended, and the exit status a shell reads that ending as.
//!
//! The executor decodes a shell's status (a tenant's out-of-memory kill seen
//! through `/bin/sh -c`), and a caller encodes an ending back into one; both
//! read the constants here, so the two spellings cannot drift apart.

use rustix::process::Signal;
use serde::{Deserialize, Serialize};

/// The signal the out-of-memory killer sends.
pub(crate) const SIGKILL: i32 = Signal::KILL.as_raw();
/// What a POSIX shell adds to a signal's number to report a child that
/// signal killed as its own exit status.
const SHELL_SIGNAL_BASE: i32 = 128;
/// The exit of a shell whose forked child was killed by `SIGKILL`.
pub(crate) const SHELL_KILLED_EXIT: i32 = SHELL_SIGNAL_BASE + SIGKILL;

/// How a process ended: exactly one of these, exactly once.
///
/// The executor's `process/exited` carries this as it is, so the wire and the
/// caller spell an ending one way: `{"kind":"exited","code":0}`,
/// `{"kind":"signaled","code":9}`, `{"kind":"timed_out"}`,
/// `{"kind":"out_of_memory"}`, `{"kind":"interrupted"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum Ending {
    /// It exited with this status.
    Exited(i32),
    /// This signal ended it.
    Signaled(i32),
    /// Its timeout elapsed and the executor killed its group.
    TimedOut,
    /// The kernel killed it because its sandbox's tenant processes ran out of
    /// memory.
    OutOfMemory,
    /// No status reached the caller: the executor or its sandbox went away,
    /// or the executor could not learn how the process ended.
    Interrupted,
}

impl Ending {
    /// How it ended, spelled as the wire's `kind`, for a log line to carry as
    /// a field; a test holds the two spellings together.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Exited(_) => "exited",
            Self::Signaled(_) => "signaled",
            Self::TimedOut => "timed_out",
            Self::OutOfMemory => "out_of_memory",
            Self::Interrupted => "interrupted",
        }
    }

    /// The exit status or the signal number, when the ending carries one.
    #[must_use]
    pub const fn code(self) -> Option<i32> {
        match self {
            Self::Exited(code) | Self::Signaled(code) => Some(code),
            Self::TimedOut | Self::OutOfMemory | Self::Interrupted => None,
        }
    }

    /// The exit status a shell reports for this ending: the process's own,
    /// or the shell's base plus the signal that ended it, saturating; an
    /// out-of-memory kill reads as the `SIGKILL` it was. `None` when no
    /// status arrived.
    #[must_use]
    pub const fn exit_code(self) -> Option<i32> {
        match self {
            Self::Exited(code) => Some(code),
            Self::Signaled(signal) => Some(SHELL_SIGNAL_BASE.saturating_add(signal)),
            Self::OutOfMemory => Some(SHELL_KILLED_EXIT),
            Self::TimedOut | Self::Interrupted => None,
        }
    }
}

#[cfg(test)]
#[path = "ending/tests.rs"]
mod tests;
