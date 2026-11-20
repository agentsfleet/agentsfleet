//! A process on a pseudo-terminal: one terminal carrying its input and its
//! merged output.
//!
//! The terminal is opened through `rustix::pty`, and the process is started
//! on its far end. Its last act before it runs its program is to lead a new
//! session the terminal controls, which also makes it its group's leader, so
//! a stop kills its group as it kills a process's on pipes. The near end is
//! read and written on threads of their own: a process that left the session
//! can hold the terminal open, and a read waiting on it, after the process
//! ended, and a runtime waits for its blocking pool when it stops.

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::fd::OwnedFd;
use std::process::Stdio;

use bytes::Bytes;
use rustix::pty::OpenptFlags;
use rustix::termios::Winsize;
use tokio::process::{Child, Command};

use super::process::{Queued, READ_CHUNK, Said};
use crate::tools::env::Stream;

/// The name a terminal's reader thread carries in a stack dump.
const READER_THREAD: &str = "guffbolt-terminal-reader";
/// The name a terminal's writer thread carries in a stack dump.
const WRITER_THREAD: &str = "guffbolt-terminal-writer";
/// The terminal's size.
const TERMINAL_SIZE: Winsize = Winsize {
    ws_row: 40,
    ws_col: 160,
    ws_xpixel: 0,
    ws_ypixel: 0,
};
/// Both ends: read and written, never the harness's controlling terminal,
/// and closed across every exec.
#[cfg(target_os = "linux")]
const END_FLAGS: OpenptFlags = OpenptFlags::RDWR
    .union(OpenptFlags::NOCTTY)
    .union(OpenptFlags::CLOEXEC);
/// Both ends, where `openpt` takes no close-on-exec flag and it is set after.
#[cfg(not(target_os = "linux"))]
const END_FLAGS: OpenptFlags = OpenptFlags::RDWR.union(OpenptFlags::NOCTTY);

/// Starts `command` on a new terminal, its output said to `said` and what is
/// queued written to it.
pub(super) fn on_terminal(mut command: Command, said: Said, queued: Queued) -> io::Result<Child> {
    let (near, far) = open_terminal()?;
    let controlling = far.try_clone()?;
    command
        .stdin(Stdio::from(far.try_clone()?))
        .stdout(Stdio::from(far.try_clone()?))
        .stderr(Stdio::from(far));
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound. `setsid` and the `TIOCSCTTY` ioctl
    // are single system calls, the second on a descriptor opened before the
    // fork, and nothing is allocated.
    unsafe { command.pre_exec(move || lead(&controlling)) };
    let child = command.spawn()?;
    // Every copy of the far end this process holds closes here, or reading
    // the near end would never see the process finish.
    drop(command);
    let reader = File::from(near.try_clone()?);
    let writer = File::from(near);
    std::thread::Builder::new()
        .name(READER_THREAD.to_owned())
        .spawn(move || read_terminal(reader, &said))?;
    std::thread::Builder::new()
        .name(WRITER_THREAD.to_owned())
        .spawn(move || write_terminal(writer, queued))?;
    Ok(child)
}

/// A new terminal: the near end the harness reads and writes, and the far
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

/// Makes the calling process lead a new session whose controlling terminal
/// is `terminal`.
fn lead(terminal: &OwnedFd) -> io::Result<()> {
    rustix::process::setsid()?;
    rustix::process::ioctl_tiocsctty(terminal)?;
    Ok(())
}

/// Reads the terminal until it closes or no one takes its output. A closed
/// terminal reads as an end on one platform and as a failure on another;
/// both stop the reading.
fn read_terminal(mut reader: File, said: &Said) {
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(failure) if failure.kind() == io::ErrorKind::Interrupted => continue,
            Err(_closed) => break,
        };
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        if said
            .send((Stream::Terminal, Bytes::copy_from_slice(chunk)))
            .is_err()
        {
            break;
        }
    }
}

/// Writes queued input to the terminal, in order, until the queue closes or
/// a write fails; a write blocks while the terminal is full.
fn write_terminal(mut writer: File, mut queued: Queued) {
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
