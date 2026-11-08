//! A model turn as the loop reads it, and how a loop's turns ended.

/// What the loop reads of a model turn; the turn itself is the embedder's.
pub trait ModelTurn: Send {
    /// The answer text the turn streamed.
    fn text(&self) -> &str;
    /// Whether the model asked for any tool call.
    fn has_calls(&self) -> bool;
    /// Whether the turn stopped at its output limit, so its calls must not run.
    fn cut(&self) -> bool;
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
}

impl<F> Ending<F> {
    /// The ending's kind, as a record names it.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Answered(_) => "answered",
            Self::Failed(_) => "failed",
            Self::Stopped => "stopped",
        }
    }
}
