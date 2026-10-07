//! What one lease's sandbox owns, and the one release every path ends with.
//!
//! [`Parts`] is the single owner of a lease's process, cgroup, workspace disk
//! and directory. [`Parts::teardown`] releases them and reports what it could
//! not; dropping `Parts` releases whatever is still held — a cancelled start, a
//! sandbox nobody destroyed — and logs what it could not. Both run the same
//! [`Parts::release`], once.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use futures_util::StreamExt as _;
use futures_util::future::OptionFuture;
use tokio::process::{Child, ChildStderr, Command};
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

use crate::cgroup::{CGROUP_PROCS, LeaseCgroup};
use crate::error::{Error, Result, cgroup, program};
use crate::host::tail;
use crate::tenant::TenantFiles;
use crate::workspace_disk::WorkspaceDisk;

/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// The name a sandbox that exited before it served is reported under.
const BWRAP: &str = "bwrap";
/// The longest error-stream line kept whole.
const LINE_MAX_BYTES: usize = 4_096;
/// How many of the sandbox's last error-stream lines a refusal quotes.
const TAIL_LINES: usize = 20;
/// The event each line the sandbox writes to its error stream is logged under.
const EVENT_SANDBOX_STDERR: &str = "sandbox_stderr";
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

    /// Takes ownership of the lease's cgroup.
    pub(super) fn adopt_cgroup(&mut self, cgroup: LeaseCgroup) -> &LeaseCgroup {
        self.cgroup.insert(cgroup)
    }

    /// Starts bubblewrap inside the cgroup whose `cgroup.procs` is `procs`,
    /// as host user `ids` when given — what a root runner passes, so nothing
    /// the sandbox does happens as host root — inheriting `tenant`, which
    /// `argv` names to the entry.
    pub(super) fn spawn(
        &mut self,
        bwrap: &Path,
        argv: Vec<OsString>,
        procs: &Path,
        tenant: TenantFiles,
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
        command
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some((uid, gid)) = ids {
            // The standard library also drops every supplementary group when
            // root sets a user, so no host group survives either.
            command.uid(uid).gid(gid);
        }
        let hook = move || {
            enter(&join)?;
            tenant.inherit()
        };
        // SAFETY: the hook runs in the child between fork and exec, where only
        // async-signal-safe calls are sound. It makes one `write` system call on
        // a file opened before the fork, then two `fcntl` calls on the tenant
        // leaf's files, all owned by the hook, and allocates nothing, so every
        // process bubblewrap starts is born inside the sandbox leaf and the
        // entry inherits the tenant leaf's two descriptors. The kernel checks
        // the write against the opener's credentials, so it holds after the
        // user change above. The files close with the command, after the spawn.
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
        // The error stream closes with the process, so its drain finishes.
        let reason = OptionFuture::from(self.stderr.take())
            .await
            .and_then(std::result::Result::ok)
            .unwrap_or_default();
        waited.map_or_else(Error::from, |status| program(BWRAP, status, reason))
    }

    /// Whether bubblewrap is still running.
    pub(super) fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    /// Ends every process, then releases the cgroup, the disk and the
    /// directory, off the async runtime: removing a cgroup waits for the
    /// kernel to reap.
    pub(super) async fn teardown(mut self) -> Result<()> {
        let event = EVENT_TEARDOWN_STARTED;
        tracing::debug!(lease_id = self.lease_id.as_str(), event);
        if let Some(drain) = self.stderr.take() {
            drain.abort();
        }
        // The lease's name comes back with the outcome, so the release is
        // logged here, where the caller's subscriber is.
        let (released, lease_id) = tokio::task::spawn_blocking(move || {
            (self.release(), std::mem::take(&mut self.lease_id))
        })
        .await?;
        log_release(&lease_id, released.as_ref().err());
        released
    }

    /// Moves everything still held into parts of its own, leaving these empty.
    fn take(&mut self) -> Self {
        Self {
            lease_id: std::mem::take(&mut self.lease_id),
            dir: self.dir.take(),
            disk: self.disk.take(),
            cgroup: self.cgroup.take(),
            child: self.child.take(),
            stderr: self.stderr.take(),
        }
    }

    /// Releases everything still held, in the one order that works: processes
    /// die before their cgroup goes, and the disk is unmounted before its
    /// directory is removed. A disk that will not unmount keeps its directory,
    /// image and all, for the boot sweep: an attached loop device is never
    /// left on a file nobody can name.
    fn release(&mut self) -> Result<()> {
        let mut first = None;
        let mut keep = |step: Result<()>| {
            if let Err(error) = step {
                first.get_or_insert(error);
            }
        };
        if let Some(cgroup) = &self.cgroup {
            keep(cgroup.kill());
        }
        if let Some(mut child) = self.child.take() {
            // Usually already gone with its cgroup; a sandbox that died on its
            // own is not a teardown failure.
            if matches!(child.try_wait(), Ok(None)) {
                keep(child.start_kill().map_err(Error::from));
            }
        }
        if let Some(cgroup) = self.cgroup.take() {
            keep(cgroup.remove());
        }
        let disk_released = self.disk.take().is_none_or(|disk| {
            let released = disk.release();
            let ok = released.is_ok();
            keep(released);
            ok
        });
        if let Some(dir) = self.dir.take().filter(|_| disk_released) {
            keep(fs::remove_dir_all(dir).map_err(Error::from));
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for Parts {
    fn drop(&mut self) {
        // After a teardown nothing is left; after a cancelled start or a
        // sandbox nobody destroyed, this is the only cleanup there will be.
        let held = self.dir.is_some()
            || self.disk.is_some()
            || self.cgroup.is_some()
            || self.child.is_some();
        if !held {
            return;
        }
        let mut owned = self.take();
        let mut release = move || {
            let released = owned.release();
            log_release(&owned.lease_id, released.as_ref().err());
        };
        // A cgroup can take seconds to empty. On a runtime of either flavor
        // the release goes to its blocking pool, logging where this thread
        // logs, so a drop never holds a worker; with no runtime there is no
        // worker to hold. A release a stopping runtime never runs is left to
        // the boot sweep.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let logs = tracing::dispatcher::get_default(Clone::clone);
                drop(runtime.spawn_blocking(move || {
                    tracing::dispatcher::with_default(&logs, release);
                }));
            }
            Err(_no_runtime) => release(),
        }
    }
}

/// Logs how a release ended.
fn log_release(lease_id: &str, failed: Option<&Error>) {
    match failed {
        None => {
            let event = EVENT_TEARDOWN_COMPLETED;
            tracing::debug!(lease_id, event);
        }
        Some(error) => {
            let error_code = error.code().as_str();
            let reason = error.to_string();
            let event = EVENT_TEARDOWN_FAILED;
            tracing::warn!(
                lease_id,
                error_code,
                reason,
                event,
                "a sandbox left something behind for the boot sweep"
            );
        }
    }
}

/// Moves the calling process into the cgroup whose `cgroup.procs` is open as
/// `procs`.
pub(super) fn enter(procs: &File) -> std::io::Result<()> {
    rustix::io::write(procs, SELF)?;
    Ok(())
}

/// Logs each line the sandbox writes to its error stream and keeps the last
/// few, for the refusal that quotes them — cut to the same tail a host
/// program's refusal keeps.
///
/// An over-long line is skipped, not fatal: the codec discards it to its
/// newline, the stream pauses once with `None`, and reading resumes — so the
/// reason a sandbox gives after a flood is still the one quoted. Only a `None`
/// that follows no overrun is the end of the stream.
async fn drain(stream: ChildStderr) -> String {
    let mut lines = FramedRead::new(stream, LinesCodec::new_with_max_length(LINE_MAX_BYTES));
    let mut last = VecDeque::with_capacity(TAIL_LINES);
    let mut overran = false;
    loop {
        match lines.next().await {
            Some(Ok(line)) => {
                overran = false;
                let event = EVENT_SANDBOX_STDERR;
                tracing::debug!(line, event);
                if last.len() == TAIL_LINES {
                    last.pop_front();
                }
                last.push_back(line);
            }
            Some(Err(LinesCodecError::MaxLineLengthExceeded)) => overran = true,
            None if overran => overran = false,
            Some(Err(LinesCodecError::Io(_))) | None => break,
        }
    }
    tail(Vec::from(last).join("\n").as_bytes())
}
