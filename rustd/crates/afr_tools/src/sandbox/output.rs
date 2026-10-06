//! What a process said, as the model reads it: output in the order it
//! arrived, cut in the middle to a budget, and a status line last.
//!
//! What a call has not read yet is kept as a head and a tail
//! (`afr_executor`'s edges), and a call reads only once it is done waiting,
//! so it holds those edges at most, whatever the process said meanwhile; the
//! next call starts from where this one stopped. This budget is the model's,
//! and smaller. Output that fits it carries a marker where each gap fell;
//! output that does not carries one marker counting every byte not shown. A
//! process whose output was still open when it ended says so before its
//! status. The status line comes last so the thread's `output_head`
//! stays the command's own first lines, and it is worded as Codex words it,
//! so a model trained on Codex's harness reads it unprompted.

use afr_executor::{EDGE_BYTES, Ending, Process, ProcessEvent};
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
/// What a call whose process ended with its output still open reads last:
/// nothing written after the drain gave it up is shown.
const ABANDONED: &str =
    "... the process ended with its output still open; what was written after is not shown ...";
/// The most tokens a call reads back, whatever the model asks: the two edges
/// a call can hold, at four bytes a token; Codex's
/// `UNIFIED_EXEC_OUTPUT_MAX_TOKENS`.
const OUTPUT_TOKENS_MAX: usize = 2 * EDGE_BYTES / BYTES_PER_TOKEN;

/// A process's output as it arrived, whichever stream carried it, and the
/// bytes dropped unread between its head and tail.
#[derive(Debug, Default)]
pub(super) struct Collected {
    bytes: Vec<u8>,
    /// Where in `bytes` each gap fell, and the bytes it dropped.
    gaps: Vec<(usize, u64)>,
    /// Whether output still arriving when the process ended was left behind.
    abandoned: bool,
}

impl Collected {
    /// Waits for `process` to end, then reads it: `Interrupted` when its
    /// events finished with no ending, as they do when the executor goes away.
    pub(super) async fn read_to_end(&mut self, process: &mut Process) -> Ending {
        process.events.finished().await;
        self.arrived(process).unwrap_or(Ending::Interrupted)
    }

    /// Waits until `process` ends or `deadline` passes, then reads what
    /// arrived; `None` while it still runs.
    pub(super) async fn until(
        &mut self,
        process: &mut Process,
        deadline: Instant,
    ) -> Option<Ending> {
        let _still_running = tokio::time::timeout_at(deadline, process.events.finished()).await;
        self.arrived(process)
    }

    /// Takes what already arrived, without waiting: the ending when it was
    /// among it, and `Interrupted` for events that finished with none.
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
            ProcessEvent::Omitted { bytes } => {
                self.gaps.push((self.bytes.len(), bytes));
                None
            }
            ProcessEvent::Ended {
                ending,
                output_abandoned,
            } => {
                self.abandoned = output_abandoned;
                Some(ending)
            }
        }
    }

    /// The output as text, at most `budget` bytes of it. What fits is shown
    /// whole, a marker where each gap fell; what does not is its first and
    /// last halves around one marker counting every byte not shown. Output
    /// left behind at the process's end is said last.
    pub(super) fn text(&self, budget: usize) -> String {
        let whole = String::from_utf8_lossy(&self.bytes);
        let shown = if whole.len() > budget {
            self.cut(&whole, budget)
        } else if self.gaps.is_empty() {
            whole.into_owned()
        } else {
            self.with_gaps()
        };
        if self.abandoned {
            with_line(shown, ABANDONED)
        } else {
            shown
        }
    }

    /// The whole output, a marker on a line of its own where each gap fell.
    fn with_gaps(&self) -> String {
        let mut shown = String::new();
        let mut from = 0;
        for (at, bytes) in &self.gaps {
            let run = self.bytes.get(from..*at).unwrap_or_default();
            shown.push_str(&String::from_utf8_lossy(run));
            shown = with_line(shown, &marker(*bytes));
            shown.push('\n');
            from = *at;
        }
        let rest = self.bytes.get(from..).unwrap_or_default();
        shown.push_str(&String::from_utf8_lossy(rest));
        shown
    }

    /// The first and last halves of `whole` around one marker, which counts
    /// the bytes cut between them and every gap.
    fn cut(&self, whole: &str, budget: usize) -> String {
        let (head, tail) = edges(whole, budget);
        let cut = whole.len() - head.len() - tail.len();
        let omitted = self.gaps.iter().fold(
            u64::try_from(cut).unwrap_or(u64::MAX),
            |sum, (_at, bytes)| sum.saturating_add(*bytes),
        );
        let marked = with_line(head.to_owned(), &marker(omitted));
        if tail.is_empty() {
            marked
        } else {
            format!("{marked}\n{tail}")
        }
    }
}

/// How `omitted` bytes not shown read.
fn marker(omitted: u64) -> String {
    format!("... {omitted} bytes omitted ...")
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
        .min(OUTPUT_TOKENS_MAX)
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
