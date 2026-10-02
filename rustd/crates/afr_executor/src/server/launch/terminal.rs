//! The pseudo-terminal launcher: one terminal carrying input and merged output.

use std::io::{self, Read as _, Write as _};

use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;

use super::{Input, Launcher, OUTPUT_BACKLOG, Plan, Spawned, Status, leader, missing_pipe};
use crate::api::Stream;
use crate::edges::Chunk;

/// The most a terminal read takes at once.
const READ_CHUNK_BYTES: usize = 64 * 1024;
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
    fn launch(&self, plan: &Plan) -> io::Result<Spawned> {
        let pair = native_pty_system()
            .openpty(TERMINAL_SIZE)
            .map_err(io::Error::other)?;
        let child = pair
            .slave
            .spawn_command(terminal_command(plan))
            .map_err(io::Error::other)?;
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
        tokio::task::spawn_blocking(move || read_terminal(reader, &sender));
        let exit = Box::pin(async move {
            let waited = tokio::task::spawn_blocking(move || child.wait()).await;
            waited
                .ok()
                .and_then(Result::ok)
                .map_or(Status::Lost, Status::from)
        });
        Ok(Spawned {
            pid,
            input: Box::new(TerminalInput(Some(writer))),
            exit,
            output,
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

/// Reads the terminal until it closes, on a blocking thread.
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

/// A terminal process's input. The writer blocks, so each write moves it to a
/// blocking thread and back.
pub(super) struct TerminalInput(pub(super) Option<Box<dyn io::Write + Send>>);

#[async_trait::async_trait]
impl Input for TerminalInput {
    async fn write(&mut self, data: Bytes) -> io::Result<()> {
        let mut writer = self.0.take().ok_or_else(missing_pipe)?;
        let (writer, written) = tokio::task::spawn_blocking(move || {
            let written = writer.write_all(&data).and_then(|()| writer.flush());
            (writer, written)
        })
        .await?;
        self.0 = Some(writer);
        written
    }
}
