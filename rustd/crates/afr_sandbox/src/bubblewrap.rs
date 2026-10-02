//! The bubblewrap command line one lease's sandbox starts with.
//!
//! Pure: what is mounted where, which namespaces are new, and what the process
//! inside may not keep. Codex's `linux-sandbox` launches bubblewrap the same
//! way; this one adds `--disable-userns` and `--clearenv`, and hardens further
//! from the inside (`crate::harden`).

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Where the workspace disk appears inside the sandbox.
pub const SANDBOX_WORKSPACE: &str = "/workspace";
/// Where the executor's socket directory appears inside the sandbox.
pub const SANDBOX_RUN_DIR: &str = "/run/agentsfleet";
/// The executor's socket, inside the sandbox.
pub const SANDBOX_SOCKET: &str = "/run/agentsfleet/executor.sock";
/// The socket's file name, the same on both sides of the bind.
pub const SOCKET_NAME: &str = "executor.sock";
/// Where the runner binary is bound, read-only, inside the sandbox.
pub const SANDBOX_ENTRY: &str = "/opt/agentsfleet/agentsfleet-runner";
/// The runner's sub-command that hardens and serves inside the sandbox.
pub const SANDBOX_SUBCOMMAND: &str = "sandbox";
/// The only environment variable a sandboxed process starts with.
const PATH_VARIABLE: &str = "PATH";
/// Its value: the toolbox's program directories.
const SANDBOX_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Where `/proc` is mounted fresh for the new process namespace.
const PROC: &str = "/proc";
/// Where a minimal device tree is mounted.
const DEV: &str = "/dev";
/// Where a private scratch file system is mounted.
const TMP: &str = "/tmp";
/// Where a private runtime directory is mounted, beneath which the executor's
/// socket directory is bound.
const RUN: &str = "/run";

/// A fresh namespace of every kind a tool call could share with the host.
const NAMESPACES: [&str; 5] = [
    "--unshare-user",
    "--unshare-pid",
    "--unshare-ipc",
    "--unshare-uts",
    "--unshare-net",
];
/// What the process inside may not keep or do.
const RESTRICTIONS: [&str; 6] = [
    "--disable-userns",
    "--cap-drop",
    "ALL",
    "--clearenv",
    "--die-with-parent",
    "--new-session",
];
/// Binds a host path read-only.
const RO_BIND: &str = "--ro-bind";
/// Binds a host path read-write.
const BIND: &str = "--bind";
/// Mounts a fresh `/proc`.
const PROC_FLAG: &str = "--proc";
/// Mounts a minimal `/dev`.
const DEV_FLAG: &str = "--dev";
/// Mounts an empty `tmpfs`.
const TMPFS_FLAG: &str = "--tmpfs";
/// Sets one environment variable.
const SETENV_FLAG: &str = "--setenv";
/// Sets the working directory.
const CHDIR_FLAG: &str = "--chdir";
/// Ends bubblewrap's own options.
const END_OF_OPTIONS: &str = "--";

/// The host paths a sandbox is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout<'a> {
    /// The mounted toolbox, which becomes the read-only root.
    pub toolbox: &'a Path,
    /// The mounted workspace disk.
    pub workspace: &'a Path,
    /// The directory the executor's socket is made in.
    pub run_dir: &'a Path,
    /// The binary that hardens and serves inside.
    pub entry: &'a Path,
    /// What it is told after its own name; `sandbox` for the runner.
    pub entry_args: &'a [OsString],
}

/// Bubblewrap's arguments for `layout`, ending with the entry and its own.
#[must_use]
pub fn arguments(layout: &Layout<'_>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = NAMESPACES
        .into_iter()
        .chain(RESTRICTIONS)
        .map(OsString::from)
        .collect();
    let mut flag = |parts: &[&OsStr]| argv.extend(parts.iter().map(|part| part.to_os_string()));
    flag(&[RO_BIND.as_ref(), layout.toolbox.as_os_str(), "/".as_ref()]);
    flag(&[PROC_FLAG.as_ref(), PROC.as_ref()]);
    flag(&[DEV_FLAG.as_ref(), DEV.as_ref()]);
    flag(&[TMPFS_FLAG.as_ref(), TMP.as_ref()]);
    // A private `/run`, so the socket directory's mount point exists whatever
    // the image's own `/run` holds; image builders empty it.
    flag(&[TMPFS_FLAG.as_ref(), RUN.as_ref()]);
    flag(&[
        BIND.as_ref(),
        layout.workspace.as_os_str(),
        SANDBOX_WORKSPACE.as_ref(),
    ]);
    flag(&[
        BIND.as_ref(),
        layout.run_dir.as_os_str(),
        SANDBOX_RUN_DIR.as_ref(),
    ]);
    flag(&[
        RO_BIND.as_ref(),
        layout.entry.as_os_str(),
        SANDBOX_ENTRY.as_ref(),
    ]);
    flag(&[
        SETENV_FLAG.as_ref(),
        PATH_VARIABLE.as_ref(),
        SANDBOX_PATH.as_ref(),
    ]);
    flag(&[CHDIR_FLAG.as_ref(), SANDBOX_WORKSPACE.as_ref()]);
    flag(&[END_OF_OPTIONS.as_ref(), SANDBOX_ENTRY.as_ref()]);
    argv.extend(layout.entry_args.iter().cloned());
    argv
}

#[cfg(test)]
mod tests;
