//! The pseudo-terminal launcher: one terminal carrying input and merged output.

use std::io::{self, Read as _, Write as _};

use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::{mpsc, oneshot};

use super::{Input, Launcher, OUTPUT_BACKLOG, Plan, Spawned, ending_of, leader, missing_pipe};
use crate::api::Stream;
use crate::edges::Chunk;
use crate::error::{self, Error, Result};

/// The most a terminal read takes at once.
const READ_CHUNK_BYTES: usize = 64 * 1024;
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
    fn launch(&self, plan: &Plan) -> Result<Spawned> {
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
                failure.downcast::<io::Error>().map_or_else(
                    |other| error::program_unavailable(other.to_string()),
                    Error::from,
                )
            })?;
        // The child holds its own copy of the terminal's far end; this one
        // must go, or reading the near end never sees the child finish.
        drop(pair.slave);
        let child: Box<dyn portable_pty::Child> = child;
        let mut child = child
            .downcast::<std::process::Child>()
            .map_err(|_other| missing_pipe())?;
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
        let exit = Box::pin(async move {
            let waited = tokio::task::spawn_blocking(move || child.wait()).await;
            ending_of(waited.unwrap_or_else(|stopped| Err(io::Error::other(stopped))))
        });
        Ok(Spawned {
            pid,
            input: Box::new(TerminalInput::start(writer)?),
            exit,
            output,
            readers: Vec::new(),
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

/// One write for the terminal's writer thread, and where to say how it went.
pub(super) type Write = (Bytes, oneshot::Sender<io::Result<()>>);

/// A terminal process's input, written by a thread of its own: the writer
/// blocks while the terminal is full.
pub(super) struct TerminalInput(pub(super) std::sync::mpsc::Sender<Write>);

impl TerminalInput {
    /// Starts the thread that writes to `writer`, in order, until a write
    /// fails or the input is dropped.
    pub(super) fn start(mut writer: Box<dyn io::Write + Send>) -> io::Result<Self> {
        let (sender, queued) = std::sync::mpsc::channel::<Write>();
        std::thread::Builder::new()
            .name(WRITER_THREAD.to_owned())
            .spawn(move || {
                for (data, done) in queued {
                    let written = writer.write_all(&data).and_then(|()| writer.flush());
                    let failed = written.is_err();
                    let _caller_gone = done.send(written);
                    if failed {
                        break;
                    }
                }
            })?;
        Ok(Self(sender))
    }
}

#[async_trait::async_trait]
impl Input for TerminalInput {
    async fn write(&mut self, data: Bytes) -> io::Result<()> {
        let (done, written) = oneshot::channel();
        self.0
            .send((data, done))
            .map_err(|_stopped| missing_pipe())?;
        written.await.map_err(|_stopped| missing_pipe())?
    }
}
