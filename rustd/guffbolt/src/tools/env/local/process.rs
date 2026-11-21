//! One process `LocalEnv` starts, from its start to its end.
//!
//! A process holds only the variables its spawn names and a search path:
//! nothing of the harness's own environment, such as a key in it, reaches a
//! command. It leads a process group of its own, so a stop, a timeout and a
//! watcher dropped before the process ended kill everything it started, not
//! only itself. Its output is fed on as it arrives, and its ending is said
//! last.

use std::io;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use crate::tools::env::{Ending, Feed, Spawn, Stream};

/// How long a process's output may stay open after it exits, held by
/// something it started, before its output is given up.
const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// How much one read of a pipe takes.
pub(super) const READ_CHUNK: usize = 16 * 1024;
/// The signal a kill ends a process with.
const KILLED: i32 = 9;
/// The variable a program is looked up through.
const PATH_VARIABLE: &str = "PATH";
/// The search path a process gets when its spawn names none.
pub(super) const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// Where a process's output goes as it is read.
pub(super) type Said = mpsc::UnboundedSender<(Stream, Bytes)>;
/// What the calls queue for a process's input.
pub(super) type Queued = mpsc::UnboundedReceiver<Bytes>;

/// `program` with `args`, run in `cwd`, holding a search path and then the
/// variables `spawn` names, which may replace it, and nothing else.
pub(super) fn command(program: &str, args: &[String], cwd: &Path, spawn: &Spawn) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env(PATH_VARIABLE, DEFAULT_PATH)
        .envs(spawn.environment())
        .kill_on_drop(true);
    command
}

/// Starts `command` on three pipes as the leader of a new process group,
/// its output said to `said` and what is queued written to its input.
pub(super) fn on_pipes(mut command: Command, said: Said, queued: Queued) -> io::Result<Child> {
    #[cfg(unix)]
    command.process_group(0);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(stdin) = child.stdin.take() {
        tokio::spawn(feed_input(stdin, queued));
    }
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_pipe(stdout, Stream::Stdout, said.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_pipe(stderr, Stream::Stderr, said));
    }
    Ok(child)
}

/// Writes what the calls queue to the process's input, until it closes it.
async fn feed_input(mut stdin: ChildStdin, mut queued: Queued) {
    while let Some(data) = queued.recv().await {
        if stdin.write_all(&data).await.is_err() {
            break;
        }
    }
}

/// Hands each chunk `pipe` says to the watcher, until it closes.
async fn read_pipe(mut pipe: impl AsyncRead + Unpin, stream: Stream, said: Said) {
    let mut buffer = vec![0; READ_CHUNK];
    while let Ok(read) = pipe.read(&mut buffer).await {
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        if said.send((stream, Bytes::copy_from_slice(chunk))).is_err() {
            break;
        }
    }
}

/// One process from its start to its end: its output fed on, a kill or its
/// timeout carried out on its whole group, and its ending said last.
pub(super) struct Watch {
    pub(super) child: Child,
    pub(super) said: mpsc::UnboundedReceiver<(Stream, Bytes)>,
    pub(super) feed: Feed,
    pub(super) kill: Arc<Notify>,
    pub(super) deadline: Option<Instant>,
}

impl Watch {
    /// Watches the process to its end. A kill or the timeout that comes
    /// while something the leader started still holds the output, after the
    /// leader exited, kills that group too.
    pub(super) async fn run(mut self) {
        let mut group = Group::new(leader_of(&self.child));
        let mut cause: Option<Ending> = None;
        let status = loop {
            let deadline = self.deadline;
            tokio::select! {
                status = self.child.wait() => {
                    group.reaped();
                    break status.ok();
                }
                () = self.kill.notified() => {
                    group.kill(&mut self.child);
                    cause.get_or_insert(Ending::Signaled(KILLED));
                }
                () = sleep_until(deadline) => {
                    group.kill(&mut self.child);
                    cause.get_or_insert(Ending::TimedOut);
                    self.deadline = None;
                }
                Some((stream, data)) = self.said.recv() => self.feed.output(stream, data),
            }
        };
        let drained = Instant::now() + DRAIN_GRACE;
        let mut abandoned = false;
        loop {
            let deadline = self.deadline;
            tokio::select! {
                next = self.said.recv() => match next {
                    Some((stream, data)) => self.feed.output(stream, data),
                    None => break,
                },
                () = self.kill.notified() => {
                    group.kill(&mut self.child);
                    cause.get_or_insert(Ending::Signaled(KILLED));
                }
                () = sleep_until(deadline) => {
                    group.kill(&mut self.child);
                    cause.get_or_insert(Ending::TimedOut);
                    self.deadline = None;
                }
                () = tokio::time::sleep_until(drained) => {
                    abandoned = true;
                    break;
                }
            }
        }
        group.settled();
        let ending = cause.unwrap_or_else(|| status.map_or(Ending::Interrupted, ending_of));
        self.feed.end(ending, abandoned);
    }
}

/// The group a process leads: killed whole on a stop or a timeout, and when
/// its watcher is dropped first, such as when the runtime stops.
///
/// Once the leader is reaped, the group's number stays the group's only
/// while a member lives, such as a background process still holding the
/// output, so it is signalled once more at most: by a kill or the timeout
/// while the output drains, or by a watcher dropped then. It is never
/// signalled after the output settled.
#[derive(Debug)]
struct Group {
    leader: Option<Leader>,
    reaped: bool,
}

impl Group {
    const fn new(leader: Option<Leader>) -> Self {
        Self {
            leader,
            reaped: false,
        }
    }

    /// Kills every process in the group, the leader included; after the
    /// leader was reaped, this is the group's last signal.
    fn kill(&mut self, child: &mut Child) {
        let leader = if self.reaped {
            self.leader.take()
        } else {
            self.leader
        };
        if let Some(leader) = leader {
            kill_group(leader);
        }
        // The leader's own kill also covers a platform with no groups.
        let _signalled = child.start_kill();
    }

    /// The leader was reaped: its number is the group's only while a member
    /// lives.
    const fn reaped(&mut self) {
        self.reaped = true;
    }

    /// The output settled after the leader was reaped: the group is never
    /// signalled again, since its number may now be another's.
    const fn settled(&mut self) {
        self.leader = None;
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(leader) = self.leader.take() {
            kill_group(leader);
        }
    }
}

/// A group's leader, which is also the group's number.
#[cfg(unix)]
type Leader = rustix::process::Pid;

/// The group `child` leads.
#[cfg(unix)]
fn leader_of(child: &Child) -> Option<Leader> {
    child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw)
}

/// Kills every process in `leader`'s group.
#[cfg(unix)]
fn kill_group(leader: Leader) {
    // A group already gone has nothing left to kill.
    let _gone = rustix::process::kill_process_group(leader, rustix::process::Signal::KILL);
}

/// A platform without process groups names no leader.
#[cfg(not(unix))]
type Leader = std::convert::Infallible;

#[cfg(not(unix))]
fn leader_of(_child: &Child) -> Option<Leader> {
    None
}

#[cfg(not(unix))]
fn kill_group(leader: Leader) {
    match leader {}
}

/// Sleeps until `deadline`, or forever when there is none.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// How a process that exited with `status` ended.
fn ending_of(status: ExitStatus) -> Ending {
    if let Some(code) = status.code() {
        return Ending::Exited(code);
    }
    signal_of(status).map_or(Ending::Interrupted, Ending::Signaled)
}

#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

#[cfg(not(unix))]
fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}
