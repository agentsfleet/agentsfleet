//! One model turn, read from its stream, and how a loop's turns ended.

use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::model::{Chunk, TextKind, Usage};
use crate::transcript::{Call, Replay};

/// What one model turn produced.
#[derive(Debug, Default)]
pub struct Turn {
    /// The answer text, as the model wrote it: the embedder scrubs it before
    /// the transcript keeps it.
    pub text: String,
    /// The calls the model asked for, in order.
    pub calls: Vec<Call>,
    /// What the turn spent.
    pub usage: Usage,
    /// What the provider needs back on the next turn.
    pub replay: Replay,
    /// The turn stopped at its output limit, so its calls must not run.
    pub cut: bool,
    /// The stream sent the turn's end.
    pub finished: bool,
}

/// Hears a model's stream while a turn is read: the embedder shows its text
/// live and counts what it spent. A loop's [`crate::Observe`]r hears its
/// turns this way.
///
/// Every method defaults to hearing nothing. Each takes `&self`, because the
/// loop reads a turn while the request it sent still borrows the embedder.
pub trait Listen {
    /// The model streamed `text` of `kind`.
    fn text(&self, _kind: TextKind, _text: &str) {}

    /// The turn reported what it spent, as each report arrives.
    fn usage(&self, _usage: Usage) {}

    /// The stream ended whole; a stream that failed or was dropped never
    /// reaches here.
    fn end(&self) {}
}

impl Turn {
    /// Reads `stream` to its end, telling `listener` each piece as it
    /// arrives. An error ends the turn there; dropping the stream closes the
    /// provider's connection.
    ///
    /// # Errors
    /// The model's stream failed.
    pub async fn read<E, L: Listen + ?Sized>(
        mut stream: BoxStream<'_, Result<Chunk, E>>,
        listener: &L,
    ) -> Result<Self, E> {
        let mut turn = Self::default();
        while let Some(chunk) = stream.next().await {
            match chunk? {
                Chunk::Text { kind, text } => {
                    listener.text(kind, &text);
                    if kind == TextKind::Answer {
                        turn.text.push_str(&text);
                    }
                }
                Chunk::Call(call) => turn.calls.push(call),
                Chunk::Usage(usage) => {
                    turn.usage += usage;
                    listener.usage(usage);
                }
                Chunk::End(end) => {
                    turn.finished = true;
                    turn.replay = end.replay;
                    turn.cut = end.cut;
                }
            }
        }
        listener.end();
        Ok(turn)
    }
}

/// How a loop's turns ended, `F` being the embedder's failure.
#[derive(Debug)]
pub enum Ending<F> {
    /// The model answered without a call, and no input was waiting: the
    /// answering turns' text, joined with [`crate::ANSWER_JOIN`].
    Answered(String),
    /// The model call failed, and the run with it.
    Failed(F),
    /// The loop was stopped before it could answer.
    Stopped,
    /// A call asked the person a question, and the run waits for the answer:
    /// the embedder delivers the question, and resumes the run with
    /// [`crate::DriveState::resume`] once it has an answer.
    Waiting(Waiting),
}

/// A question a call asked the person, which the run waits on.
///
/// Saved with the run's [`crate::DriveState`], so a run resumed later knows
/// which call the answer belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waiting {
    /// The call that asked, whose result the answer becomes.
    pub call_id: String,
    /// What it asked.
    pub question: String,
}

impl<F> Ending<F> {
    /// The ending's kind, as a record names it.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Answered(_) => "answered",
            Self::Failed(_) => "failed",
            Self::Stopped => "stopped",
            Self::Waiting(_) => "waiting",
        }
    }
}

#[cfg(test)]
mod tests;
