//! The pseudo-terminal launcher: one terminal carrying input and merged output.
//!
//! The terminal is opened through `rustix::pty` and its process started by the
//! standard library on the terminal's far end, so it takes the same placement
//! hook a process on pipes does. Its last act before exec is to lead a new
//! session the terminal controls, which also makes it its group's leader.

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use bytes::Bytes;
use rustix::process::{Pid, Signal, kill_process_group};
use rustix::pty::OpenptFlags;
use rustix::termios::Winsize;
use tokio::sync::mpsc;

use super::{Launcher, OUTPUT_BACKLOG, Placement, Plan, Spawned, ending_of, leader, tenant};
use crate::api::Stream;
use crate::edges::Chunk;
use crate::error::Result;
use crate::protocol::READ_CHUNK_BYTES;

/// The name a terminal's reader thread carries in a stack dump.
const READER_THREAD: &str = "executor-terminal-reader";
/// The name a terminal's writer thread carries in a stack dump.
const WRITER_THREAD: &str = "executor-terminal-writer";
/// The pseudo-terminal's geometry.
const TERMINAL_SIZE: Winsize = Winsize {
    ws_row: 40,
    ws_col: 160,
    ws_xpixel: 0,
    ws_ypixel: 0,
};
/// Both ends: read and written, never a controlling terminal of the
/// executor's, and closed across every exec.
#[cfg(target_os = "linux")]
const END_FLAGS: OpenptFlags = OpenptFlags::RDWR
    .union(OpenptFlags::NOCTTY)
    .union(OpenptFlags::CLOEXEC);
/// Both ends, where `openpt` takes no close-on-exec flag and it is set after.
#[cfg(not(target_os = "linux"))]
const END_FLAGS: OpenptFlags = OpenptFlags::RDWR.union(OpenptFlags::NOCTTY);

/// One pseudo-terminal carrying input and merged output.
pub(super) struct Terminal;

impl Launcher for Terminal {
    fn launch(
        &self,
        plan: &Plan,
        placement: &Arc<dyn Placement>,
        input: mpsc::Receiver<Bytes>,
    ) -> Result<Spawned> {
        let (near, far) = open_terminal()?;
        let mut child = start(plan, placement, far)?;
        let pid = leader(Some(child.id()))?;
        let reader = File::from(near.try_clone()?);
        let writer = File::from(near);
        let (sender, output) = mpsc::channel(OUTPUT_BACKLOG);
        // Threads of their own rather than the runtime's blocking pool: a
        // descendant that left the session can keep the terminal open, and a
        // read or a write on it waiting, long after the process has been
        // reported ended — and a runtime waits for its pool when it stops.
        std::thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || read_terminal(Box::new(reader), &sender))?;
        std::thread::Builder::new()
            .name(WRITER_THREAD.to_owned())
            .spawn(move || write_terminal(Box::new(writer), input))?;
        let exit = Box::pin(async move {
            // Dropped before the wait ends — its task aborted, its runtime
            // stopping — the leader's group goes with it, as a pipe leader's
            // does through `kill_on_drop`.
            let unwaited = KillOnDrop(Some(pid));
            let waited = tokio::task::spawn_blocking(move || child.wait()).await;
            unwaited.disarm();
            ending_of(waited.unwrap_or_else(|stopped| Err(io::Error::other(stopped))))
        });
        Ok(Spawned {
            pid,
            exit,
            output,
            tasks: Vec::new(),
        })
    }
}

/// A new terminal: the near end the executor reads and writes, and the far
/// end a process runs on.
fn open_terminal() -> io::Result<(OwnedFd, OwnedFd)> {
    let near = rustix::pty::openpt(END_FLAGS)?;
    rustix::pty::grantpt(&near)?;
    rustix::pty::unlockpt(&near)?;
    let far = far_end(&near)?;
    rustix::termios::tcsetwinsize(&far, TERMINAL_SIZE)?;
    Ok((near, far))
}

/// The far end of the terminal whose near end is `near`, opened through the
/// near end itself, so no path under `/dev/pts` is looked up.
#[cfg(target_os = "linux")]
fn far_end(near: &OwnedFd) -> io::Result<OwnedFd> {
    Ok(rustix::pty::ioctl_tiocgptpeer(near, END_FLAGS)?)
}

/// The far end of the terminal whose near end is `near`, opened by its name.
#[cfg(not(target_os = "linux"))]
fn far_end(near: &OwnedFd) -> io::Result<OwnedFd> {
    use rustix::fs::{Mode, OFlags};
    use rustix::io::FdFlags;

    rustix::io::fcntl_setfd(near, FdFlags::CLOEXEC)?;
    let name = rustix::pty::ptsname(near, Vec::new())?;
    let flags = OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC;
    Ok(rustix::fs::open(name.as_c_str(), flags, Mode::empty())?)
}

/// Starts `plan` on the terminal's far end `far`, placed by `placement`, as
/// the leader of a new session the terminal controls. Every copy of `far`
/// this process holds is closed on return, or reading the near end would
/// never see the process finish.
fn start(plan: &Plan, placement: &Arc<dyn Placement>, far: OwnedFd) -> Result<Child> {
    let controlling = far.try_clone()?;
    let mut command = Command::new(&plan.program);
    command
        .args(&plan.arguments)
        .env_clear()
        .envs(&plan.env)
        .current_dir(&plan.cwd)
        .stdin(Stdio::from(far.try_clone()?))
        .stdout(Stdio::from(far.try_clone()?))
        .stderr(Stdio::from(far));
    tenant::place(&mut command, placement)?;
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound. `setsid` and the `TIOCSCTTY` ioctl
    // are single system calls, the second on a descriptor opened before the
    // fork, and nothing is allocated.
    unsafe { command.pre_exec(move || lead(&controlling)) };
    Ok(command.spawn()?)
}

/// Makes the calling process lead a new session whose controlling terminal is
/// `terminal`.
fn lead(terminal: &OwnedFd) -> io::Result<()> {
    rustix::process::setsid()?;
    rustix::process::ioctl_tiocsctty(terminal)?;
    Ok(())
}

/// Kills a group when dropped while armed: the leader of a terminal whose wait
/// was abandoned.
pub(super) struct KillOnDrop(pub(super) Option<Pid>);

impl KillOnDrop {
    /// The wait finished; the leader is reaped and its number may be reused.
    pub(super) fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            // A group already gone has nothing left to kill.
            let _gone = kill_process_group(pid, Signal::KILL);
        }
    }
}

/// Reads the terminal until it closes or no one takes its output.
pub(super) fn read_terminal(mut reader: Box<dyn io::Read + Send>, sender: &mpsc::Sender<Chunk>) {
    let mut buffer = vec![0; READ_CHUNK_BYTES];
    while let Ok(read @ 1..) = reader.read(&mut buffer) {
        let data = Bytes::copy_from_slice(buffer.get(..read).unwrap_or_default());
        if sender
            .blocking_send(Chunk {
                stream: Stream::Terminal,
                data,
            })
            .is_err()
        {
            break;
        }
    }
}

/// Writes queued input to the terminal, in order, on a thread of its own —
/// the write blocks while the terminal is full — until the queue closes or a
/// write fails; then the queue closes, and later writes are refused.
pub(super) fn write_terminal(
    mut writer: Box<dyn io::Write + Send>,
    mut queued: mpsc::Receiver<Bytes>,
) {
    while let Some(data) = queued.blocking_recv() {
        if writer
            .write_all(&data)
            .and_then(|()| writer.flush())
            .is_err()
        {
            break;
        }
    }
}
