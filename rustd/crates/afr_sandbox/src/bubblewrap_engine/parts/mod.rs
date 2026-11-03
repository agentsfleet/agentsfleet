//! What one lease's sandbox owns, and the one release every path ends with.
//!
//! [`Parts`] is the single owner of a lease's process, cgroup, egress scope,
//! workspace disk and directory. [`Parts::teardown`] releases them and reports what it could
//! not; dropping `Parts` releases whatever is still held — a cancelled start, a
//! sandbox nobody destroyed — and logs what it could not. Both run the same
//! [`Parts::release`], once.

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::cgroup::{CGROUP_PROCS, Freezer, LeaseCgroup};
use crate::egress::Scope;
use crate::engine::Killed;
use crate::error::{EgressRefusal, Error, Result, cgroup, egress_refused, program, unconfined};
use crate::network::Allowlist;
use crate::tenant::Handover;
use crate::workspace_disk::WorkspaceDisk;

use super::NO_CGROUP;
use super::held::off_runtime;
use super::names::Names;
use super::stderr::drain;

mod teardown;

/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// The name a sandbox that exited before it served is reported under.
const BWRAP: &str = "bwrap";
/// The event a teardown's start is logged under.
const EVENT_TEARDOWN_STARTED: &str = "sandbox_teardown_started";
/// The event a teardown that removed everything is logged under.
const EVENT_TEARDOWN_COMPLETED: &str = "sandbox_teardown_completed";
/// The event a teardown that left something behind is logged under.
const EVENT_TEARDOWN_FAILED: &str = "sandbox_teardown_failed";

/// Everything one lease's sandbox owns; built a piece at a time, released once.
#[derive(Debug)]
pub(super) struct Parts {
    lease_id: String,
    dir: Option<PathBuf>,
    disk: Option<WorkspaceDisk>,
    cgroup: Option<LeaseCgroup>,
    egress: Option<Scope>,
    child: Option<Child>,
    stderr: Option<JoinHandle<String>>,
}

impl Parts {
    /// Nothing built yet, in the lease's own directory.
    pub(super) fn new(lease_id: &str, dir: PathBuf) -> Self {
        Self {
            lease_id: lease_id.to_owned(),
            dir: Some(dir),
            disk: None,
            cgroup: None,
            egress: None,
            child: None,
            stderr: None,
        }
    }

    /// The lease's directory.
    pub(super) fn dir(&self) -> &Path {
        self.dir.as_deref().unwrap_or(Path::new(""))
    }

    /// The lease's `workspace/` on its disk, once it has one.
    pub(super) fn workspace(&self) -> &Path {
        self.disk
            .as_ref()
            .map_or(Path::new(""), WorkspaceDisk::workspace)
    }

    /// The lease's `tmp/` on its disk, once it has one.
    pub(super) fn tmp(&self) -> &Path {
        self.disk.as_ref().map_or(Path::new(""), WorkspaceDisk::tmp)
    }

    /// Takes ownership of the lease's workspace disk.
    pub(super) fn adopt_disk(&mut self, disk: WorkspaceDisk) -> &WorkspaceDisk {
        self.disk.insert(disk)
    }

    /// The freezer of the lease's cgroup, once it has one.
    pub(super) fn freezer(&self) -> Option<Freezer> {
        self.cgroup.as_ref().map(LeaseCgroup::freezer)
    }

    /// Takes ownership of the lease's cgroup.
    pub(super) fn adopt_cgroup(&mut self, cgroup: LeaseCgroup) -> &LeaseCgroup {
        self.cgroup.insert(cgroup)
    }

    /// Takes ownership of the scope joining the lease's sandbox to the host.
    pub(super) fn adopt_egress(&mut self, scope: Scope) {
        self.egress = Some(scope);
    }

    /// Holds the lease's scope to `allowlist` in place of the allowlist it
    /// was built to, off the async runtime since netlink blocks, then renders
    /// the sandbox's names to match. The scope comes back whatever the swap
    /// did, a panic included ([`off_runtime`]), so a refused swap leaves it as
    /// it was, for the release.
    ///
    /// # Errors
    /// No scope was built, the kernel refused the swap, the swap panicked, or
    /// the names would not render.
    pub(super) async fn reallow(&mut self, allowlist: &Allowlist) -> Result<()> {
        let span = tracing::Span::current();
        let next = allowlist.clone();
        off_runtime(&mut self.egress, move |scope| {
            span.in_scope(|| scope.reallow(&next))
        })
        .await?
        .ok_or_else(|| egress_refused(EgressRefusal::NoScope))??;
        Names::rewrite(self.dir(), allowlist)
    }

    /// Starts bubblewrap inside the cgroup whose `cgroup.procs` is `procs`,
    /// as host user `ids` when given — what a root runner passes, so nothing
    /// the sandbox does happens as host root — inheriting `handover`'s
    /// descriptors, which `argv` names to the entry.
    pub(super) fn spawn(
        &mut self,
        bwrap: &Path,
        argv: Vec<OsString>,
        procs: &Path,
        handover: Handover,
        ids: Option<(u32, u32)>,
    ) -> Result<()> {
        // `create` is a no-op on a cgroup file system, which publishes the file
        // with the cgroup; on a plain directory it lets the engine be proven
        // without root.
        let join = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(procs)
            .map_err(cgroup(CGROUP_PROCS))?;
        let mut command = Command::new(bwrap);
        // `--clearenv` empties only what the entry inherits. bubblewrap itself,
        // and the monitor it leaves on the host, would otherwise carry the root
        // runner's environment, credential included, readable through
        // `/proc/<pid>/environ` by any host process sharing the sandbox's user.
        // It needs none: the entry is an absolute path and the log level
        // travels by `--setenv` (`crate::bubblewrap`).
        command
            .args(argv)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some((uid, gid)) = ids {
            // The standard library also drops every supplementary group when
            // root sets a user, so no host group survives either.
            command.uid(uid).gid(gid);
        }
        let hook = move || in_child(&join, &handover);
        // SAFETY: the hook runs in the child between fork and exec, where only
        // async-signal-safe calls are sound. [`in_child`] makes one `write`
        // system call on a file opened before the fork, then three `fcntl`
        // calls on the tenant leaf's files and the entry's end of the
        // executor's pair, all owned by the hook, and allocates nothing, so
        // every process bubblewrap starts is born inside the sandbox leaf and
        // the entry inherits all three. The kernel checks the write against
        // the opener's credentials, so it holds after the user change above.
        // The files close with the command, after the spawn.
        unsafe { command.pre_exec(hook) };
        let mut child = command.spawn()?;
        self.stderr = child
            .stderr
            .take()
            .map(|stream| tokio::spawn(drain(stream)));
        self.child = Some(child);
        Ok(())
    }

    /// Resolves once bubblewrap exits, with the reason it gave.
    pub(super) async fn exited(&mut self) -> Error {
        let Some(child) = self.child.as_mut() else {
            return std::future::pending().await;
        };
        let waited = child.wait().await;
        // The error stream closes with the process, so its drain finishes. It
        // is awaited where it stands and cleared only once done: a caller that
        // stops waiting here, as the ready check's race does, leaves the
        // stream for its next call rather than losing it.
        let reason = match self.stderr.as_mut() {
            Some(drain) => drain.await.unwrap_or_default(),
            None => String::new(),
        };
        self.stderr = None;
        waited.map_or_else(Error::from, |status| program(BWRAP, status, reason))
    }

    /// Whether bubblewrap is still running.
    pub(super) fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    /// Ends every process in the sandbox and waits, bounded, for its cgroup
    /// to empty. The disk, the cgroup and the egress rules stay for the
    /// teardown.
    pub(super) async fn kill(&self) -> Killed {
        let Some(cgroup) = &self.cgroup else {
            return Killed::Unsettled(unconfined(NO_CGROUP));
        };
        match cgroup.kill_and_drain().await {
            Ok(()) => Killed::Settled,
            Err(failure) => Killed::Unsettled(failure),
        }
    }
}

/// Moves the calling process into the cgroup whose `cgroup.procs` is open as
/// `procs`.
pub(super) fn enter(procs: &File) -> std::io::Result<()> {
    rustix::io::write(procs, SELF)?;
    Ok(())
}

/// What bubblewrap's child does between fork and exec: moves into the
/// sandbox leaf whose `cgroup.procs` is open as `join`, then lets
/// `handover`'s descriptors through the exec.
pub(super) fn in_child(join: &File, handover: &Handover) -> std::io::Result<()> {
    enter(join)?;
    handover.inherit()
}
