//! What a sandbox writes to its error stream: each line logged, and the last
//! few kept for the refusal that quotes them.

use std::collections::VecDeque;

use afr_image::tail;
use futures_util::StreamExt as _;
use tokio::process::ChildStderr;
use tokio_util::codec::{FramedRead, LinesCodec};

/// How many of the sandbox's last error-stream lines a refusal quotes.
const TAIL_LINES: usize = 20;
/// The event each line the sandbox writes to its error stream is logged under.
const EVENT_SANDBOX_STDERR: &str = "sandbox_stderr";

/// Logs each line the sandbox writes to its error stream and keeps the last
/// few, for the refusal that quotes them.
pub(super) async fn drain(stream: ChildStderr) -> String {
    let mut lines = FramedRead::new(stream, LinesCodec::new());
    let mut last = VecDeque::with_capacity(TAIL_LINES);
    while let Some(Ok(line)) = lines.next().await {
        let event = EVENT_SANDBOX_STDERR;
        tracing::debug!(line, event);
        if last.len() == TAIL_LINES {
            last.pop_front();
        }
        last.push_back(line);
    }
    tail(Vec::from(last).join("\n").as_bytes())
}
