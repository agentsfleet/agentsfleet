//! The tools that run a process through the environment, none of which
//! starts one on the host itself.
//!
//! `shell` runs one command to its end; `exec_command` starts a process the
//! model drives across calls and `write_stdin` feeds it, in the shape of
//! Codex's unified exec (`codex-rs/core/src/unified_exec/`). What the three
//! share lives here and beside them: the shell and the environment every
//! command starts with, how output and endings read (`output`), a command
//! run to its end (`oneshot`) and the processes a run keeps open
//! (`sessions`). `git` runs the environment's git.

use crate::tool::ToolOutput;
use crate::tools::code;
use crate::tools::env::{EnvError, Spawn};

pub mod exec_session;
pub mod git;
pub mod oneshot;
pub mod output;
pub mod sessions;
pub mod shell;

pub use self::exec_session::{ExecCommand, WriteStdin};
pub use self::git::Git;
pub use self::shell::Shell;

/// The shell every command runs under.
const SHELL_PROGRAM: &str = "/bin/sh";
/// The flag that hands the shell its command.
const SHELL_COMMAND_FLAG: &str = "-c";
/// The locale every command runs in, so its output decodes as UTF-8.
const UTF8_LOCALE: &str = "C.UTF-8";
/// The pager every command is given: one that never waits for a reader.
const NO_PAGER: &str = "cat";
/// The variables every command starts with: Codex's `UNIFIED_EXEC_ENV`.
///
/// Its own marker is left out. No pager waits on a terminal nobody reads,
/// and no colour code reaches the model.
pub const COMMAND_ENV: [(&str, &str); 9] = [
    ("NO_COLOR", "1"),
    ("TERM", "dumb"),
    ("LANG", UTF8_LOCALE),
    ("LC_CTYPE", UTF8_LOCALE),
    ("LC_ALL", UTF8_LOCALE),
    ("COLORTERM", ""),
    ("PAGER", NO_PAGER),
    ("GIT_PAGER", NO_PAGER),
    ("GH_PAGER", NO_PAGER),
];

/// `script` as the shell runs it: `sh -c script`, at the root, with
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

/// What a call reads back when the environment failed it: it could not be
/// reached or would not do it, in its own words.
pub(super) fn unavailable(failure: &EnvError) -> ToolOutput {
    ToolOutput::failed(code::SANDBOX_UNAVAILABLE, failure.message())
}

/// A span in whole milliseconds, saturated rather than wrapped.
pub(crate) fn saturating_millis(span: std::time::Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}
