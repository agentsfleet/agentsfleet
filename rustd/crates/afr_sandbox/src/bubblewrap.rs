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

use crate::network::{SANDBOX_HOSTS, SANDBOX_RESOLV_CONF};
use crate::tenant::TenantDescriptors;

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
/// Where the workspace disk's `tmp/` is bound: scratch space that shares the
/// disk's limit, so filling it answers `ENOSPC` rather than taking the
/// lease's memory.
pub(crate) const SANDBOX_TMP: &str = "/tmp";
/// Where a private runtime directory is mounted, beneath which the executor's
/// socket directory is bound.
const RUN: &str = "/run";

/// A fresh namespace of every kind a tool call could share with the host, the
/// network aside ([`NetworkLayout`]). The cgroup one hides the host's cgroup
/// tree, and with it the lease's path.
const NAMESPACES: [&str; 5] = [
    "--unshare-user",
    "--unshare-pid",
    "--unshare-ipc",
    "--unshare-uts",
    "--unshare-cgroup",
];
/// A network namespace of the sandbox's own, holding nothing but loopback
/// until the engine joins it to the host.
const UNSHARE_NET: &str = "--unshare-net";
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
/// Binds a host path read-only when it exists, and skips it when it does not.
const RO_BIND_TRY: &str = "--ro-bind-try";
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
/// Sets the size, in bytes, of the `tmpfs` the next flag mounts.
const SIZE_FLAG: &str = "--size";
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

/// Whether a sandbox has a network namespace of its own, and where the names
/// it resolves come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkLayout<'a> {
    /// The host's network namespace, and the host's own resolver files at the
    /// same paths, so a name resolves as it does on the host.
    Host,
    /// A namespace of its own with nothing but loopback, and the image's
    /// resolver files.
    Isolated,
    /// A namespace of its own, which the engine joins to the host once the
    /// sandbox runs, and the resolver files rendered for its allowlist.
    Allowed {
        /// The rendered `/etc/hosts`, on the host.
        hosts: &'a Path,
        /// The resolver-less `/etc/resolv.conf`, on the host.
        resolv_conf: &'a Path,
    },
}

impl NetworkLayout<'_> {
    /// Whether the sandbox gets a network namespace of its own.
    const fn unshares(self) -> bool {
        !matches!(self, Self::Host)
    }
}

/// The host paths a sandbox is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout<'a> {
    /// The mounted toolbox, which becomes the read-only root.
    pub toolbox: &'a Path,
    /// The workspace disk's `workspace/` directory.
    pub workspace: &'a Path,
    /// The workspace disk's `tmp/` directory.
    pub tmp: &'a Path,
    /// The directory the executor's socket is made in.
    pub run_dir: &'a Path,
    /// The binary that hardens and serves inside.
    pub entry: &'a Path,
    /// What it is told after its own name; `sandbox` for the runner.
    pub entry_args: &'a [OsString],
    /// The tenant leaf's descriptors, named to the entry after its own
    /// arguments.
    pub tenant: TenantDescriptors,
    /// What `/dev/shm` may hold, in bytes (`Limits::shared_memory_bytes`).
    pub shared_memory_bytes: u64,
    /// The log level the process inside logs at, passed through the cleared
    /// environment when the runner has one set.
    pub log_level: Option<&'a OsStr>,
    /// What its network reaches, and where its names come from.
    pub network: NetworkLayout<'a>,
}

/// Bubblewrap's arguments for `layout`, ending with the entry, its own, and
/// the tenant leaf's descriptors.
#[must_use]
pub fn arguments(layout: &Layout<'_>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = NAMESPACES
        .into_iter()
        .chain(layout.network.unshares().then_some(UNSHARE_NET))
        .chain(RESTRICTIONS)
        .map(OsString::from)
        .collect();
    mount(&mut argv, layout);
    names(&mut argv, layout.network);
    // The executor puts its own default `PATH` on every process it starts;
    // the sandbox's entry needs none.
    if let Some(level) = layout.log_level {
        flag(
            &mut argv,
            &[SETENV_FLAG.as_ref(), LOG_LEVEL_VAR.as_ref(), level],
        );
    }
    let (uid, gid) = (SANDBOX_UID.to_string(), SANDBOX_GID.to_string());
    flag(
        &mut argv,
        &[
            UID_FLAG.as_ref(),
            uid.as_ref(),
            GID_FLAG.as_ref(),
            gid.as_ref(),
        ],
    );
    flag(
        &mut argv,
        &[CHDIR_FLAG.as_ref(), SANDBOX_WORKSPACE.as_ref()],
    );
    flag(
        &mut argv,
        &[END_OF_OPTIONS.as_ref(), SANDBOX_ENTRY.as_ref()],
    );
    argv.extend(layout.entry_args.iter().cloned());
    argv.extend(layout.tenant.arguments());
    argv
}

/// Appends what the sandbox sees, in the order bubblewrap stacks it: the
/// read-only toolbox as `/`, fresh `/proc` and `/dev`, private shared memory,
/// the disk's `tmp/`, a private `/run`, the disk's `workspace/`, the socket
/// directory, and the entry.
fn mount(argv: &mut Vec<OsString>, layout: &Layout<'_>) {
    flag(
        argv,
        &[RO_BIND.as_ref(), layout.toolbox.as_os_str(), "/".as_ref()],
    );
    flag(argv, &[PROC_FLAG.as_ref(), PROC.as_ref()]);
    flag(argv, &[DEV_FLAG.as_ref(), DEV.as_ref()]);
    let shared_memory = layout.shared_memory_bytes.to_string();
    flag(
        argv,
        &[
            PERMS_FLAG.as_ref(),
            SHARED_MEMORY_MODE.as_ref(),
            SIZE_FLAG.as_ref(),
            shared_memory.as_ref(),
            TMPFS_FLAG.as_ref(),
            DEV_SHM.as_ref(),
        ],
    );
    flag(
        argv,
        &[BIND.as_ref(), layout.tmp.as_os_str(), SANDBOX_TMP.as_ref()],
    );
    // A private `/run`, so the socket directory's mount point exists whatever
    // the image's own `/run` holds; image builders empty it.
    flag(argv, &[TMPFS_FLAG.as_ref(), RUN.as_ref()]);
    for (flag_name, host, inside) in [
        (BIND, layout.workspace, SANDBOX_WORKSPACE),
        (BIND, layout.run_dir, SANDBOX_RUN_DIR),
        (RO_BIND, layout.entry, SANDBOX_ENTRY),
    ] {
        flag(
            argv,
            &[flag_name.as_ref(), host.as_os_str(), inside.as_ref()],
        );
    }
}

/// Appends the resolver files `network` names, over the image's own.
fn names(argv: &mut Vec<OsString>, network: NetworkLayout<'_>) {
    let binds: [(&str, &OsStr, &str); 2] = match network {
        // The host's own files, at the paths the image has them: a host
        // without one keeps the image's.
        NetworkLayout::Host => [
            (RO_BIND_TRY, SANDBOX_HOSTS.as_ref(), SANDBOX_HOSTS),
            (
                RO_BIND_TRY,
                SANDBOX_RESOLV_CONF.as_ref(),
                SANDBOX_RESOLV_CONF,
            ),
        ],
        NetworkLayout::Isolated => return,
        NetworkLayout::Allowed { hosts, resolv_conf } => [
            (RO_BIND, hosts.as_os_str(), SANDBOX_HOSTS),
            (RO_BIND, resolv_conf.as_os_str(), SANDBOX_RESOLV_CONF),
        ],
    };
    for (flag_name, host, inside) in binds {
        flag(argv, &[flag_name.as_ref(), host, inside.as_ref()]);
    }
}

/// Appends one flag and its values.
fn flag(argv: &mut Vec<OsString>, parts: &[&OsStr]) {
    argv.extend(parts.iter().map(|part| part.to_os_string()));
}

#[cfg(test)]
mod tests;
