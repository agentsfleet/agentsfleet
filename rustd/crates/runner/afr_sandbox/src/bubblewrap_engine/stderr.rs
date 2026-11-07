//! What a sandbox writes to its error stream: each line logged, and the last
//! few kept for the refusal that quotes them.

use std::collections::VecDeque;

use futures_util::StreamExt as _;
use tokio::process::ChildStderr;
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

use crate::host::tail;

/// The longest error-stream line kept whole.
const LINE_MAX_BYTES: usize = 4_096;
/// How many of the sandbox's last error-stream lines a refusal quotes.
const TAIL_LINES: usize = 20;
/// The event each line the sandbox writes to its error stream is logged under.
const EVENT_SANDBOX_STDERR: &str = "sandbox_stderr";

/// Logs each line the sandbox writes to its error stream and keeps the last
/// few, for the refusal that quotes them — cut to the same tail a host
/// program's refusal keeps.
///
/// An over-long line is skipped, not fatal: the codec discards it to its
/// newline, the stream pauses once with `None`, and reading resumes — so the
/// reason a sandbox gives after a flood is still the one quoted. Only a `None`
/// that follows no overrun is the end of the stream.
pub(super) async fn drain(stream: ChildStderr) -> String {
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
