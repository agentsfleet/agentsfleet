//! The executor: processes and files inside one sandbox.
//!
//! One executor runs inside each lease's sandbox as `agentsfleet-runner
//! sandbox`, and the supervisor reaches it over a Unix socket — the shape a
//! microVM's vsock surfaces on the host, so a later engine attaches the same
//! client unchanged (`docs/architecture/runner_execution.md` §Process model).
//!
//! # The seam
//!
//! [`Executor`] is what the agent loop calls. It is a trait object because the
//! loop holds whichever engine's executor its lease got, and because a test can
//! drive the loop with no sandbox at all. Each spawned [`Process`] carries its
//! own event channel, owned by the caller that spawned it, so no table of open
//! processes is shared behind a lock: the connection task owns the senders and
//! the caller owns the receivers.

pub mod error;

mod api;

pub use self::api::{
    DirEntry, Ending, EntryKind, Executor, FileContent, Process, ProcessEvent, ProcessId, Spawn,
    Stream,
};
pub use self::error::{Error, Result};
