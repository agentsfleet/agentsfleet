//! The run's sandbox as `guffbolt`'s environment: every process and file
//! call a generic tool makes goes to the guest agent inside the run's
//! sandbox, and none starts a process on the host.
//!
//! The guest agent's own types and refusals are translated one to one, so a
//! tool reads what it read when it called the guest agent itself: the same
//! spawn, plus the git identity a commit made in the sandbox names, the same
//! `/workspace` root, the same wire message for a failure and the same log
//! code and reason for a refused kill.
//!
//! A run's tools are built when the run starts, before its sandbox is ready,
//! so they hold a [`LazySandboxEnv`]: each call gets the guest agent from the
//! run's source first, waiting for the build when it has not finished, and a
//! run with no sandbox reads why as the environment's own refusal.

mod lazy;
mod search;

use std::pin::Pin;
use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use af_guest_wire::WORKSPACE_ROOT;
use af_guest_wire::timing::{DRAIN_GRACE, KILL_GRACE};
use afr_guest_client::GuestAgent;
use bytes::Bytes;
use guffbolt::tools::env::{
    Browser, DirEntry, Ending, EntryKind, Env, EnvError, EnvErrorKind, EnvFuture, Events,
    FileContent, Found, Listing, Process, ProcessEvent, ProcessId, Search, Spawn, Stream,
};

use crate::sandbox::{GIT_IDENTITY_EMAIL, GIT_IDENTITY_NAME};

pub use self::lazy::LazySandboxEnv;

/// Local reply delivery allowed beyond the native cleanup and drain bounds.
const REPLY_SLACK: Duration = Duration::from_secs(1);
/// How long past a command's timeout the guest agent takes to report its
/// ending: the group's kill grace, the pipes' drain grace and the reply.
const ENDING_GRACE: Duration = KILL_GRACE
    .saturating_add(DRAIN_GRACE)
    .saturating_add(REPLY_SLACK);
const _: () = assert!(ENDING_GRACE.as_nanos() <= crate::ENGINE_STOP_GRACE.as_nanos());
// Unconfigured, the harness and the guest agent hold one read size and one edge;
// configured, the tools read the guest agent's through `Env`.
const _: () = assert!(guffbolt::tools::env::MAX_READ_BYTES == af_guest_wire::MAX_READ_BYTES);
const _: () = assert!(guffbolt::tools::env::EDGE_BYTES == af_guest_wire::EDGE_BYTES);
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

/// The run's guest agent, as a generic tool's environment.
#[derive(Debug, Clone, Copy)]
pub struct SandboxEnv<'a> {
    agent: &'a dyn GuestAgent,
}

impl<'a> SandboxEnv<'a> {
    /// The environment of the sandbox `agent` serves.
    #[must_use]
    pub const fn new(agent: &'a dyn GuestAgent) -> Self {
        Self { agent }
    }
}

/// `spawn` as the guest agent reads it, the git identity added.
fn spawn_of(spawn: &Spawn) -> afr_guest_client::Spawn {
    let (program, args) = spawn
        .argv()
        .split_first()
        .map_or(("", &[][..]), |(program, args)| (program.as_str(), args));
    let mut native = afr_guest_client::Spawn::program(program).args(args);
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

/// A guest agent failure as a tool reads it: sorted, in the guest agent's own
/// words, and logged under the registry's code.
fn failed(failure: &af_guest_wire::Error) -> EnvError {
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

const fn stream_of(stream: af_guest_wire::Stream) -> Stream {
    match stream {
        af_guest_wire::Stream::Stdout => Stream::Stdout,
        af_guest_wire::Stream::Stderr => Stream::Stderr,
        af_guest_wire::Stream::Terminal => Stream::Terminal,
    }
}

const fn ending_of(ending: af_guest_wire::Ending) -> Ending {
    match ending {
        af_guest_wire::Ending::Exited(code) => Ending::Exited(code),
        af_guest_wire::Ending::Signaled(signal) => Ending::Signaled(signal),
        af_guest_wire::Ending::TimedOut => Ending::TimedOut,
        af_guest_wire::Ending::OutOfMemory => Ending::OutOfMemory,
        af_guest_wire::Ending::Interrupted => Ending::Interrupted,
    }
}

fn event_of(event: afr_guest_client::ProcessEvent) -> ProcessEvent {
    match event {
        afr_guest_client::ProcessEvent::Output { stream, data } => ProcessEvent::Output {
            stream: stream_of(stream),
            data,
        },
        afr_guest_client::ProcessEvent::Omitted { bytes } => ProcessEvent::Omitted { bytes },
        afr_guest_client::ProcessEvent::Ended {
            ending,
            output_abandoned,
        } => ProcessEvent::Ended {
            ending: ending_of(ending),
            output_abandoned,
        },
    }
}

const fn entry_kind_of(kind: af_guest_wire::EntryKind) -> EntryKind {
    match kind {
        af_guest_wire::EntryKind::File => EntryKind::File,
        af_guest_wire::EntryKind::Directory => EntryKind::Directory,
        af_guest_wire::EntryKind::Symlink => EntryKind::Symlink,
        af_guest_wire::EntryKind::Other => EntryKind::Other,
    }
}

/// A sandboxed process's events, read through the guest agent's own bounded
/// store.
#[derive(Debug)]
struct Sandboxed(afr_guest_client::Events);

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
                .agent
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
            let id = afr_guest_client::ProcessId::new(process.get());
            self.agent
                .write(id, data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        Box::pin(async move {
            let id = afr_guest_client::ProcessId::new(process.get());
            self.agent
                .kill(id)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent> {
        Box::pin(async move {
            let read = self
                .agent
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
            self.agent
                .write_file(path, data)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn delete_file<'a>(&'a self, path: &'a str) -> EnvFuture<'a, ()> {
        Box::pin(async move {
            self.agent
                .delete_file(path)
                .await
                .map_err(|failure| failed(&failure))
        })
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing> {
        Box::pin(async move {
            let listing = self
                .agent
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

    fn search<'a>(&'a self, search: &'a Search) -> EnvFuture<'a, Found> {
        Box::pin(async move {
            self.agent
                .search(&search::search_of(search))
                .await
                .map(search::found_of)
                .map_err(|failure| failed(&failure))
        })
    }

    fn ending_grace(&self) -> Duration {
        ENDING_GRACE
    }

    fn max_read_bytes(&self) -> u64 {
        self.agent.sizes().max_read_bytes
    }

    fn edge_bytes(&self) -> usize {
        self.agent.sizes().edge_bytes
    }

    fn browser(&self) -> Result<&dyn Browser, &str> {
        Err(WAITS_FOR_FIRECRACKER)
    }
}

#[cfg(test)]
mod tests;
