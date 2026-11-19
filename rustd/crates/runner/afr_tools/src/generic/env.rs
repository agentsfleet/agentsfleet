//! The lease's sandbox as `guffbolt`'s environment: every process and file
//! call a generic tool makes goes to the executor inside the lease's
//! bubblewrap sandbox, and none starts a process on the host.
//!
//! The executor's own types and refusals are translated one to one, so a
//! tool reads what it read when it called the executor itself: the same
//! spawn, plus the git identity a commit made in the sandbox names, the same
//! `/workspace` root, the same wire message for a failure and the same log
//! code and reason for a refused kill.

use std::pin::Pin;
use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use afr_executor::timing::{CALL_DEADLINE, DRAIN_GRACE, KILL_GRACE};
use afr_executor::{Executor, WORKSPACE_ROOT};
use bytes::Bytes;
use guffbolt::tools::env::{
    Browser, DirEntry, Ending, EntryKind, Env, EnvError, EnvErrorKind, EnvFuture, Events,
    FileContent, Listing, Process, ProcessEvent, ProcessId, Spawn, Stream,
};
use guffbolt::tools::process::oneshot::{TIMEOUT_DEFAULT, TIMEOUT_MAX};

use crate::sandbox::{GIT_IDENTITY_EMAIL, GIT_IDENTITY_NAME};

/// Local reply delivery allowed beyond the native cleanup and drain bounds.
const REPLY_SLACK: Duration = Duration::from_secs(1);
/// How long past a command's timeout the executor takes to report its
/// ending: the group's kill grace, the pipes' drain grace and the reply.
const ENDING_GRACE: Duration = KILL_GRACE
    .saturating_add(DRAIN_GRACE)
    .saturating_add(REPLY_SLACK);
const _: () = assert!(
    TIMEOUT_DEFAULT.saturating_add(ENDING_GRACE).as_nanos() < af_core::timing::LEASE_TTL.as_nanos()
);
const _: () = assert!(
    CALL_DEADLINE
        .saturating_add(TIMEOUT_MAX)
        .saturating_add(ENDING_GRACE)
        .as_nanos()
        < af_core::timing::MAX_RUNTIME.as_nanos()
);
const _: () = assert!(ENDING_GRACE.as_nanos() <= af_core::timing::ENGINE_STOP_GRACE.as_nanos());
// The tools word their limits by the harness's figures, which must be the
// executor's.
const _: () = assert!(guffbolt::tools::env::MAX_READ_BYTES == afr_executor::MAX_READ_BYTES);
const _: () = assert!(guffbolt::tools::env::EDGE_BYTES == afr_executor::EDGE_BYTES);
/// What each browser tool reads back after its name.
const WAITS_FOR_FIRECRACKER: &str =
    "waits for the Firecracker engine: Chromium cannot start inside this sandbox";
/// The identity every command's commits name, beside the harness's own
/// variables: `git commit` works with no `HOME` to read one from.
const GIT_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", GIT_IDENTITY_NAME),
    ("GIT_AUTHOR_EMAIL", GIT_IDENTITY_EMAIL),
    ("GIT_COMMITTER_NAME", GIT_IDENTITY_NAME),
    ("GIT_COMMITTER_EMAIL", GIT_IDENTITY_EMAIL),
];

/// The lease's executor, as a generic tool's environment.
#[derive(Debug, Clone, Copy)]
pub struct SandboxEnv<'a> {
    executor: &'a dyn Executor,
}

impl<'a> SandboxEnv<'a> {
    /// The environment of the sandbox `executor` serves.
    #[must_use]
    pub const fn new(executor: &'a dyn Executor) -> Self {
        Self { executor }
    }
}

/// `spawn` as the executor reads it, the git identity added.
fn spawn_of(spawn: &Spawn) -> afr_executor::Spawn {
    let (program, args) = spawn
        .argv()
        .split_first()
        .map_or(("", &[][..]), |(program, args)| (program.as_str(), args));
    let mut native = afr_executor::Spawn::program(program).args(args);
    if let Some(cwd) = spawn.working_directory() {
        native = native.cwd(cwd);
    }
    let variables = spawn
        .environment()
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .chain(GIT_IDENTITY);
    native = variables.fold(native, |native, (key, value)| native.env(key, value));
    if spawn.on_terminal() {
        native = native.terminal();
    }
    match spawn.time_limit() {
        Some(limit) => native.timeout(limit),
        None => native,
    }
}

/// An executor failure as a tool reads it: sorted, in the executor's own
/// words, and logged under the registry's code.
fn failed(failure: &afr_executor::Error) -> EnvError {
    let kind = if failure.is_path_refused() {
        EnvErrorKind::PathRefused
    } else if failure.is_not_found() {
        EnvErrorKind::NotFound
    } else if failure.is_unknown_process() {
        EnvErrorKind::UnknownProcess
    } else if failure.is_input_refused() {
        EnvErrorKind::InputRefused
    } else {
        EnvErrorKind::Unavailable
    };
    let Logged { error_code, reason } = failure.logged();
    EnvError::new(kind, failure.wire_message()).logged_as(error_code, reason)
}

const fn stream_of(stream: afr_executor::Stream) -> Stream {
    match stream {
        afr_executor::Stream::Stdout => Stream::Stdout,
        afr_executor::Stream::Stderr => Stream::Stderr,
        afr_executor::Stream::Terminal => Stream::Terminal,
    }
}

const fn ending_of(ending: afr_executor::Ending) -> Ending {
    match ending {
        afr_executor::Ending::Exited(code) => Ending::Exited(code),
        afr_executor::Ending::Signaled(signal) => Ending::Signaled(signal),
        afr_executor::Ending::TimedOut => Ending::TimedOut,
        afr_executor::Ending::OutOfMemory => Ending::OutOfMemory,
        afr_executor::Ending::Interrupted => Ending::Interrupted,
    }
}

fn event_of(event: afr_executor::ProcessEvent) -> ProcessEvent {
    match event {
        afr_executor::ProcessEvent::Output { stream, data } => ProcessEvent::Output {
            stream: stream_of(stream),
            data,
        },
        afr_executor::ProcessEvent::Omitted { bytes } => ProcessEvent::Omitted { bytes },
        afr_executor::ProcessEvent::Ended {
            ending,
            output_abandoned,
        } => ProcessEvent::Ended {
            ending: ending_of(ending),
            output_abandoned,
        },
    }
}

const fn entry_kind_of(kind: afr_executor::EntryKind) -> EntryKind {
    match kind {
        afr_executor::EntryKind::File => EntryKind::File,
        afr_executor::EntryKind::Directory => EntryKind::Directory,
        afr_executor::EntryKind::Symlink => EntryKind::Symlink,
        afr_executor::EntryKind::Other => EntryKind::Other,
    }
}

/// A sandboxed process's events, read through the executor's own bounded
/// store.
#[derive(Debug)]
struct Sandboxed(afr_executor::Events);

impl Events for Sandboxed {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<ProcessEvent>> + Send + '_>> {
        Box::pin(async move { self.0.recv().await.map(event_of) })
    }

    fn take_ready(&mut self) -> (Vec<ProcessEvent>, bool) {
        let (events, finished) = self.0.take_ready();
        (events.map(event_of).collect(), finished)
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn finished(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.0.finished())
    }

    fn disconnect(&mut self) {
        self.0.disconnect();
    }
}

impl Env for SandboxEnv<'_> {
    fn root(&self) -> &str {
        WORKSPACE_ROOT
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        Box::pin(async move {
            let process = self
                .executor
                .spawn(&spawn_of(spawn))
                .await
                .map_err(|failure| failed(&failure))?;
            Ok(Process {
                id: ProcessId::new(process.id.get()),
                events: Box::new(Sandboxed(process.events)),
            })
        })
    }

    fn write(&self, process: ProcessId, data: Bytes) -> EnvFuture<'_, ()> {
        Box::pin(async move {
            let id = afr_executor::ProcessId::new(process.get());
            self.executor
                .write(id, data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        Box::pin(async move {
            let id = afr_executor::ProcessId::new(process.get());
            self.executor
                .kill(id)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(async move {
            let read = self
                .executor
                .read_file(path, max_bytes)
                .await
                .map_err(|failure| failed(&failure))?;
            Ok(FileContent {
                data: read.data,
                truncated: read.truncated,
            })
        })
    }

    fn write_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            self.executor
                .write_file(path, data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn append_file<'a>(&'a self, path: &'a str, data: Bytes) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            self.executor
                .append_file(path, data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn delete_file<'a>(&'a self, path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            self.executor
                .delete_file(path)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(async move {
            let listing = self
                .executor
                .list_dir(path)
                .await
                .map_err(|failure| failed(&failure))?;
            Ok(Listing {
                entries: listing
                    .entries
                    .into_iter()
                    .map(|entry| DirEntry {
                        name: entry.name,
                        kind: entry_kind_of(entry.kind),
                        size: entry.size,
                    })
                    .collect(),
                truncated: listing.truncated,
            })
        })
    }

    fn ending_grace(&self) -> Duration {
        ENDING_GRACE
    }

    fn browser(&self) -> Result<&dyn Browser, &str> {
        Err(WAITS_FOR_FIRECRACKER)
    }
}
