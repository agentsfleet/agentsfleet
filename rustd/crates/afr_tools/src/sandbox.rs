//! The sandbox-side handlers: each runs through the lease's executor, inside
//! its sandbox, and none starts a process on the host.
//!
//! `shell` runs one command to its end; `exec_command` starts a process the
//! model drives across calls and `write_stdin` feeds it, in the shape of
//! Codex's unified exec (`codex-rs/core/src/unified_exec/`). What the three
//! share lives here and beside them: the shell and the environment every
//! command starts with, how output and endings read (`output`), a command
//! run to its end (`oneshot`), the processes a lease keeps open (`sessions`),
//! and the repositories its workspace holds (`repositories`). `git` runs the
//! toolbox's git on those.

use afr_executor::{Executor, Spawn};

use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

mod exec_session;
mod git;
mod oneshot;
mod output;
mod repositories;
#[cfg(any(test, feature = "test-util"))]
mod scripted;
mod sessions;
mod shell;

pub(crate) use self::exec_session::{ExecCommand, WriteStdin};
pub(crate) use self::git::Git;
pub use self::repositories::{CREDENTIAL_GITHUB, Checkout, checkouts};
#[cfg(any(test, feature = "test-util"))]
pub use self::scripted::{ScriptedExecutor, ScriptedProcess};
pub use self::sessions::Sessions;
pub(crate) use self::shell::Shell;

/// The shell every command runs under: the toolbox's.
const SHELL_PROGRAM: &str = "/bin/sh";
/// The flag that hands the shell its command.
const SHELL_COMMAND_FLAG: &str = "-c";
/// The locale every command runs in, so its output decodes as UTF-8.
const UTF8_LOCALE: &str = "C.UTF-8";
/// The pager every command is given: one that never waits for a reader.
const NO_PAGER: &str = "cat";
/// Who a commit made in the sandbox names as its author and committer, and
/// who the supervisor's checkout names in its reference logs.
pub const GIT_IDENTITY_NAME: &str = "agentsfleet";
/// The address that commit carries: one nobody answers.
pub const GIT_IDENTITY_EMAIL: &str = "noreply@agentsfleet.net";
/// The environment every command starts with, beside the search path the
/// executor gives it: Codex's `UNIFIED_EXEC_ENV` less its own marker, so no
/// pager waits on a terminal nobody reads and no colour code reaches the model;
/// and a git identity, so `git commit` works with no `HOME` to read one from.
const COMMAND_ENV: [(&str, &str); 13] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("LANG", UTF8_LOCALE),
    ("LC_CTYPE", UTF8_LOCALE),
    ("LC_ALL", UTF8_LOCALE),
    ("COLORTERM", ""),
    ("PAGER", NO_PAGER),
    ("GIT_PAGER", NO_PAGER),
    ("GH_PAGER", NO_PAGER),
    ("GIT_AUTHOR_NAME", GIT_IDENTITY_NAME),
    ("GIT_AUTHOR_EMAIL", GIT_IDENTITY_EMAIL),
    ("GIT_COMMITTER_NAME", GIT_IDENTITY_NAME),
    ("GIT_COMMITTER_EMAIL", GIT_IDENTITY_EMAIL),
];
/// What a sandbox-side call reads back when it was given no executor.
const NO_EXECUTOR: &str = "this call has no sandbox to run in";

/// `script` as the shell runs it: `sh -c script`, in the workspace, with
/// [`COMMAND_ENV`].
fn command(script: &str) -> Spawn {
    with_environment(
        Spawn::program(SHELL_PROGRAM)
            .arg(SHELL_COMMAND_FLAG)
            .arg(script),
    )
}

/// `spawn` with [`COMMAND_ENV`] set.
fn with_environment(spawn: Spawn) -> Spawn {
    COMMAND_ENV
        .into_iter()
        .fold(spawn, |spawn, (key, value)| spawn.env(key, value))
}

/// The lease's executor, or what a call without one reads back.
///
/// The router hands every sandbox-side call its executor; the refusal is the
/// type's other arm, for a handler called some other way.
fn executor_of<'call>(context: &ToolContext<'call, '_>) -> Result<&'call dyn Executor, ToolOutput> {
    context
        .executor
        .ok_or_else(|| ToolOutput::failed(ToolErrorCode::SandboxUnavailable, NO_EXECUTOR))
}

/// What a call reads back when the executor failed it: the sandbox could not
/// be reached or would not do it, in the executor's own words.
fn unavailable(failure: &afr_executor::Error) -> ToolOutput {
    ToolOutput::failed(ToolErrorCode::SandboxUnavailable, &failure.wire_message())
}
