//! The bubblewrap command line one lease's sandbox starts with.
//!
//! Pure: what is mounted where, which namespaces are new, and what the process
//! inside may not keep. Codex's `linux-sandbox` launches bubblewrap the same
//! way; this one adds `--disable-userns` and `--clearenv`, and hardens further
//! from the inside (`crate::harden`).
//!
//! # What the binds allow
//!
//! bubblewrap mounts every `--bind` `nosuid,nodev` unless asked otherwise, so
//! nothing on the workspace disk or in the socket directory can raise a
//! privilege or open a device. The process inside runs as [`SANDBOX_UID`],
//! never as root, and the runner starts bubblewrap itself as an unprivileged
//! host user, so a file the sandbox creates is never owned by host root.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use afd_core::env::LOG_LEVEL_VAR;

/// Where the workspace disk appears inside the sandbox: the root the executor
/// confines every path to.
pub const SANDBOX_WORKSPACE: &str = afr_executor::WORKSPACE_ROOT;
/// Where the executor's socket directory appears inside the sandbox.
pub const SANDBOX_RUN_DIR: &str = "/run/agentsfleet";
/// The socket's file name, the same on both sides of the bind.
pub const SOCKET_NAME: &str = "executor.sock";
/// The user a sandboxed process runs as, inside its user namespace.
pub const SANDBOX_UID: u32 = 1000;
/// The group a sandboxed process runs as, inside its user namespace.
pub const SANDBOX_GID: u32 = 1000;

/// The executor's socket, inside the sandbox.
#[must_use]
pub fn sandbox_socket() -> PathBuf {
    Path::new(SANDBOX_RUN_DIR).join(SOCKET_NAME)
}
/// Where the runner binary is bound, read-only, inside the sandbox.
pub const SANDBOX_ENTRY: &str = "/opt/agentsfleet/agentsfleet-runner";
/// The runner's sub-command that hardens and serves inside the sandbox.
pub const SANDBOX_SUBCOMMAND: &str = "sandbox";
/// Where `/proc` is mounted fresh for the new process namespace.
const PROC: &str = "/proc";
/// Where a minimal device tree is mounted.
const DEV: &str = "/dev";
/// Where POSIX shared memory lives: a private `tmpfs` per sandbox, which
/// Python's `multiprocessing` and Chromium need. Its pages are charged to the
/// lease's cgroup, so the memory limit covers it.
pub(crate) const DEV_SHM: &str = "/dev/shm";
/// Shared memory's mode: every user writes, and only an owner removes.
const SHARED_MEMORY_MODE: &str = "1777";
/// Where a private scratch file system is mounted.
pub(crate) const SANDBOX_TMP: &str = "/tmp";
/// Where a private runtime directory is mounted, beneath which the executor's
/// socket directory is bound.
const RUN: &str = "/run";

/// A fresh namespace of every kind a tool call could share with the host. The
/// cgroup one hides the host's cgroup tree, and with it the lease's path.
const NAMESPACES: [&str; 6] = [
    "--unshare-user",
    "--unshare-pid",
    "--unshare-ipc",
    "--unshare-uts",
    "--unshare-net",
    "--unshare-cgroup",
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
/// Sets the mode of what the next flag creates.
const PERMS_FLAG: &str = "--perms";
/// Sets one environment variable.
const SETENV_FLAG: &str = "--setenv";
/// Sets the user the process runs as inside its namespace.
const UID_FLAG: &str = "--uid";
/// Sets the group the process runs as inside its namespace.
const GID_FLAG: &str = "--gid";
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
    /// The log level the process inside logs at, passed through the cleared
    /// environment when the runner has one set.
    pub log_level: Option<&'a OsStr>,
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
    flag(&[
        PERMS_FLAG.as_ref(),
        SHARED_MEMORY_MODE.as_ref(),
        TMPFS_FLAG.as_ref(),
        DEV_SHM.as_ref(),
    ]);
    flag(&[TMPFS_FLAG.as_ref(), SANDBOX_TMP.as_ref()]);
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
    // The executor puts its own default `PATH` on every process it starts;
    // the sandbox's entry needs none.
    if let Some(level) = layout.log_level {
        flag(&[SETENV_FLAG.as_ref(), LOG_LEVEL_VAR.as_ref(), level]);
    }
    let (uid, gid) = (SANDBOX_UID.to_string(), SANDBOX_GID.to_string());
    flag(&[
        UID_FLAG.as_ref(),
        uid.as_ref(),
        GID_FLAG.as_ref(),
        gid.as_ref(),
    ]);
    flag(&[CHDIR_FLAG.as_ref(), SANDBOX_WORKSPACE.as_ref()]);
    flag(&[END_OF_OPTIONS.as_ref(), SANDBOX_ENTRY.as_ref()]);
    argv.extend(layout.entry_args.iter().cloned());
    argv
}

#[cfg(test)]
mod tests;
