//! What one lease's sandbox owns, and the one teardown every path ends with.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::os::fd::{AsRawFd as _, BorrowedFd, RawFd};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use futures_util::StreamExt as _;
use futures_util::future::OptionFuture;
use tokio::process::{Child, ChildStderr, Command};
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

use crate::cgroup::LeaseCgroup;
use crate::error::{Error, ErrorKind, Result, cgroup};
use crate::workspace_disk::WorkspaceDisk;

/// The control file a joining process writes to, as a failure names it.
const CGROUP_PROCS: &str = "cgroup.procs";
/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// The longest error-stream line kept whole.
const LINE_MAX_BYTES: usize = 4_096;
/// How many of the sandbox's last error-stream lines a refusal quotes.
const TAIL_LINES: usize = 20;
/// The event each line the sandbox writes to its error stream is logged under.
const EVENT_SANDBOX_STDERR: &str = "sandbox_stderr";
/// The event a teardown that left something behind is logged under.
const EVENT_TEARDOWN_FAILED: &str = "sandbox_teardown_failed";

/// Everything one lease's sandbox owns; built a piece at a time, torn down at once.
#[derive(Debug)]
pub(super) struct Parts {
    dir: PathBuf,
    disk: Option<WorkspaceDisk>,
    cgroup: Option<LeaseCgroup>,
    child: Option<Child>,
    stderr: Option<JoinHandle<String>>,
}

impl Parts {
    /// Nothing built yet, in the lease's own directory.
    pub(super) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            disk: None,
            cgroup: None,
            child: None,
            stderr: None,
        }
    }

    /// The lease's directory.
    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Takes ownership of the lease's workspace disk.
    pub(super) fn adopt_disk(&mut self, disk: WorkspaceDisk) -> &WorkspaceDisk {
        self.disk.insert(disk)
    }

    /// Takes ownership of the lease's cgroup.
    pub(super) fn adopt_cgroup(&mut self, cgroup: LeaseCgroup) -> &LeaseCgroup {
        self.cgroup.insert(cgroup)
    }

    /// Starts bubblewrap inside the cgroup whose `cgroup.procs` is `procs`.
    pub(super) fn spawn(&mut self, bwrap: &Path, argv: Vec<OsString>, procs: &Path) -> Result<()> {
        // `create` is a no-op on a cgroup file system, which publishes the file
        // with the cgroup; on a plain directory it lets the engine be proven
        // without root.
        let join = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(procs)
            .map_err(cgroup(CGROUP_PROCS))?;
        let descriptor = join.as_raw_fd();
        let mut command = Command::new(bwrap);
        command
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // SAFETY: the hook runs in the child between fork and exec, where only
        // async-signal-safe calls are sound. It makes one `write` system call on
        // a descriptor opened before the fork and allocates nothing, so every
        // process bubblewrap starts is born inside the lease's cgroup.
        unsafe { command.pre_exec(move || enter(descriptor)) };
        let mut child = command.spawn()?;
        drop(join);
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
        waited.map_or_else(Error::from, |status| {
            ErrorKind::Exited { status, reason }.into()
        })
    }

    /// Ends every process, then removes the cgroup, the disk and the directory.
    /// Every step runs whatever an earlier one reported; the first failure is
    /// the one returned.
    pub(super) async fn teardown(self) -> Result<()> {
        let Self {
            dir,
            disk,
            cgroup,
            child,
            stderr,
        } = self;
        let mut first = None;
        let mut keep = |step: Result<()>| {
            if let Err(error) = step {
                first.get_or_insert(error);
            }
        };
        if let Some(cgroup) = &cgroup {
            keep(cgroup.kill());
        }
        if let Some(child) = child {
            keep(end(child).await);
        }
        if let Some(drain) = stderr {
            drain.abort();
        }
        if let Some(cgroup) = cgroup {
            keep(cgroup.remove().await);
        }
        if let Some(disk) = disk {
            keep(disk.release());
        }
        keep(fs::remove_dir_all(&dir).map_err(Error::from));
        first.map_or(Ok(()), Err)
    }

    /// Tears down after a refused start, logging rather than returning what it
    /// could not remove: the refusal is the error the caller needs.
    pub(super) async fn teardown_after_refusal(self, lease_id: &str) {
        if let Err(left) = self.teardown().await {
            let reason = left.to_string();
            let event = EVENT_TEARDOWN_FAILED;
            tracing::warn!(
                lease_id,
                reason,
                event,
                "a refused sandbox left something behind"
            );
        }
    }
}

/// Kills bubblewrap unless it has already gone: the cgroup kill usually got
/// there first, and a sandbox that died on its own is not a teardown failure.
async fn end(mut child: Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    Ok(())
}

/// Moves the calling process into the cgroup open on `descriptor`.
pub(super) fn enter(descriptor: RawFd) -> std::io::Result<()> {
    // SAFETY: the descriptor was opened by the parent before the fork and is
    // still open in this child, which owns its copy until exec.
    let procs = unsafe { BorrowedFd::borrow_raw(descriptor) };
    rustix::io::write(procs, SELF)?;
    Ok(())
}

/// Logs each line the sandbox writes to its error stream and keeps the last
/// few, for the refusal that quotes them.
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
    Vec::from(last).join("\n")
}
