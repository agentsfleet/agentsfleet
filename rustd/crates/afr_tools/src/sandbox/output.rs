//! What a process said, as the model reads it: output in the order it
//! arrived, cut in the middle to a budget, and a status line last.
//!
//! The executor already keeps only a head and a tail of a process's output
//! (`afr_executor`'s edges); this budget is the model's, and smaller. The
//! marker counts the bytes both cuts dropped, so the model knows how much it
//! did not see. The status line comes last so the thread's `output_head`
//! stays the command's own first lines, and it is worded as Codex words it,
//! so a model trained on Codex's harness reads it unprompted.

use afr_executor::{Ending, Process, ProcessEvent};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::time::Instant;

use crate::runtime::ToolErrorCode;

/// The bytes one token is taken to cover: Codex's estimate.
const BYTES_PER_TOKEN: usize = 4;
/// The tokens of output a call reads back when the model names no budget:
/// Codex's `DEFAULT_MAX_OUTPUT_TOKENS`.
const OUTPUT_TOKENS_DEFAULT: usize = 10_000;
/// What a shell adds to a signal's number to report it as an exit status.
const SIGNAL_EXIT_BASE: i32 = 128;
/// How a process that exited reads.
const EXITED: &str = "Process exited with code";
/// How a session still running reads.
pub(super) const RUNNING: &str = "Process running with session ID";
/// How a process a signal ended reads.
const SIGNALED: &str = "Process killed by signal";
/// How a process the executor killed at its timeout reads.
pub(super) const TIMED_OUT: &str = "Process timed out";
/// How a process whose ending never reached the caller reads.
const INTERRUPTED: &str = "Process interrupted before its ending arrived";

/// A process's output as it arrived, whichever stream carried it, and the
/// bytes the executor dropped between its head and tail.
#[derive(Debug, Default)]
pub(super) struct Collected {
    bytes: Vec<u8>,
    omitted: u64,
}

impl Collected {
    /// Reads `process` to its end: `Interrupted` when its channel closed with
    /// no ending, as it does when the executor goes away.
    pub(super) async fn read_to_end(&mut self, process: &mut Process) -> Ending {
        while let Some(event) = process.events.recv().await {
            if let Some(ending) = self.take(event) {
                return ending;
            }
        }
        Ending::Interrupted
    }

    /// Reads `process` until it ends or `deadline` passes; `None` while it
    /// still runs.
    pub(super) async fn until(
        &mut self,
        process: &mut Process,
        deadline: Instant,
    ) -> Option<Ending> {
        loop {
            let received = tokio::time::timeout_at(deadline, process.events.recv())
                .await
                .ok()?;
            let Some(event) = received else {
                return Some(Ending::Interrupted);
            };
            if let Some(ending) = self.take(event) {
                return Some(ending);
            }
        }
    }

    /// Takes what already arrived, without waiting: the ending when it was
    /// among it, and `Interrupted` for a channel the executor closed.
    pub(super) fn arrived(&mut self, process: &mut Process) -> Option<Ending> {
        loop {
            match process.events.try_recv() {
                Ok(event) => {
                    if let Some(ending) = self.take(event) {
                        return Some(ending);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return Some(Ending::Interrupted),
            }
        }
    }

    /// Keeps one event; the ending when it was the last.
    fn take(&mut self, event: ProcessEvent) -> Option<Ending> {
        match event {
            ProcessEvent::Output { data, .. } => {
                self.bytes.extend_from_slice(&data);
                None
            }
            ProcessEvent::Ended {
                ending,
                omitted_bytes,
            } => {
                self.omitted = self.omitted.saturating_add(omitted_bytes);
                Some(ending)
            }
        }
    }

    /// The output as text, at most `budget` bytes of it: its first and last
    /// halves around a marker counting every byte not shown. Output the
    /// executor cut that still fits carries the marker last.
    pub(super) fn text(&self, budget: usize) -> String {
        let whole = String::from_utf8_lossy(&self.bytes);
        let (head, tail) = edges(&whole, budget);
        let cut = whole.len() - head.len() - tail.len();
        let omitted = self
            .omitted
            .saturating_add(u64::try_from(cut).unwrap_or(u64::MAX));
        if omitted == 0 {
            return whole.into_owned();
        }
        let marked = with_line(head.to_owned(), &format!("... {omitted} bytes omitted ..."));
        if tail.is_empty() {
            marked
        } else {
            format!("{marked}\n{tail}")
        }
    }
}

/// `text`'s first and last halves of `budget` bytes, cut on character
/// boundaries: all of it, and no tail, when it fits.
fn edges(text: &str, budget: usize) -> (&str, &str) {
    if text.len() <= budget {
        return (text, "");
    }
    let head_end = text.floor_char_boundary(budget / 2);
    let tail_start = text.ceil_char_boundary(text.len() - (budget - budget / 2));
    (
        text.get(..head_end).unwrap_or_default(),
        text.get(tail_start..).unwrap_or_default(),
    )
}

/// `text` with `line` after it, on a line of its own.
pub(super) fn with_line(mut text: String, line: &str) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text
}

/// The bytes a call reads back for `tokens` tokens, or for the default when
/// the model names none.
pub(super) fn budget(tokens: Option<usize>) -> usize {
    tokens
        .unwrap_or(OUTPUT_TOKENS_DEFAULT)
        .saturating_mul(BYTES_PER_TOKEN)
}

/// How a process that ended as `ending` reads.
pub(super) fn status(ending: Ending) -> String {
    match ending {
        Ending::Exited(code) => format!("{EXITED} {code}"),
        Ending::Signaled(signal) => format!("{SIGNALED} {signal}"),
        Ending::TimedOut => TIMED_OUT.to_owned(),
        Ending::Interrupted => INTERRUPTED.to_owned(),
    }
}

/// The exit status the ledger reads: the process's own, or a shell's spelling
/// of the signal that ended it; none when no status arrived.
pub(super) fn exit_code(ending: Ending) -> Option<i32> {
    match ending {
        Ending::Exited(code) => Some(code),
        Ending::Signaled(signal) => Some(SIGNAL_EXIT_BASE.saturating_add(signal)),
        Ending::TimedOut | Ending::Interrupted => None,
    }
}

/// The code a call fails with when its process did not end by itself.
pub(super) fn error_code(ending: Ending) -> Option<ToolErrorCode> {
    match ending {
        Ending::TimedOut => Some(ToolErrorCode::TimedOut),
        Ending::Interrupted => Some(ToolErrorCode::Interrupted),
        Ending::Exited(_) | Ending::Signaled(_) => None,
    }
}

#[cfg(test)]
#[path = "output/tests.rs"]
mod tests;
