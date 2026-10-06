//! The pseudo-terminal launcher: one terminal carrying input and merged output.

use std::io::{self, Read as _, Write as _};

use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::sync::mpsc;

use super::{Launcher, OUTPUT_BACKLOG, Plan, Spawned, ending_of, leader};
use crate::api::Stream;
use crate::edges::Chunk;
use crate::error::{self, Error, Result};
use crate::protocol::READ_CHUNK_BYTES;

/// The name a terminal's reader thread carries in a stack dump.
const READER_THREAD: &str = "executor-terminal-reader";
/// The name a terminal's writer thread carries in a stack dump.
const WRITER_THREAD: &str = "executor-terminal-writer";
/// The pseudo-terminal's geometry.
const TERMINAL_SIZE: PtySize = PtySize {
    rows: 40,
    cols: 160,
    pixel_width: 0,
    pixel_height: 0,
};

/// One pseudo-terminal carrying input and merged output.
pub(super) struct Terminal;

impl Launcher for Terminal {
    fn launch(&self, plan: &Plan, input: mpsc::Receiver<Bytes>) -> Result<Spawned> {
        let pair = native_pty_system()
            .openpty(TERMINAL_SIZE)
            .map_err(io::Error::other)?;
        let child = pair
            .slave
            .spawn_command(terminal_command(plan))
            // An operating-system failure keeps its kind; any other is the
            // launcher's own lookup refusing the program — missing, not
            // executable, a directory — which is the caller's to fix.
            .map_err(|failure| {
                failure
                    .downcast::<io::Error>()
                    .map_or_else(error::program_unavailable, Error::from)
            })?;
        // The child holds its own copy of the terminal's far end; this one
        // must go, or reading the near end never sees the child finish.
        drop(pair.slave);
        let child: Box<dyn portable_pty::Child> = child;
        let mut child = child
            .downcast::<std::process::Child>()
            .map_err(|_other| error::launch_incomplete())?;
        let pid = leader(Some(child.id()))?;
        let reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
        let writer = pair.master.take_writer().map_err(io::Error::other)?;
        let (sender, output) = mpsc::channel(OUTPUT_BACKLOG);
        // Threads of their own rather than the runtime's blocking pool: a
        // descendant that left the session can keep the terminal open, and a
        // read or a write on it waiting, long after the process has been
        // reported ended — and a runtime waits for its pool when it stops.
        std::thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || read_terminal(reader, &sender))?;
        std::thread::Builder::new()
            .name(WRITER_THREAD.to_owned())
            .spawn(move || write_terminal(writer, input))?;
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

/// The terminal's command: the plan's program, arguments, environment and
/// directory, with nothing inherited.
fn terminal_command(plan: &Plan) -> CommandBuilder {
    let mut command = CommandBuilder::new(&plan.program);
    command.args(&plan.arguments);
    command.env_clear();
    plan.env
        .iter()
        .for_each(|(key, value)| command.env(key, value));
    command.cwd(&plan.cwd);
    command
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
