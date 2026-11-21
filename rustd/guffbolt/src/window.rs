//! What one model call sends of the transcript, as the embedder fits it.
//!
//! The loop keeps every message in the [`Transcript`](crate::Transcript) and
//! knows nothing of a model's window. Before each model call it asks the
//! embedder for that call's [`Window`], the way pi runs `transformContext`
//! before each call: the transcript as it is, or a copy that fits, with what
//! no longer fits summarized or dropped. A window lives for one call, so
//! fitting it never edits what the loop keeps.

use crate::transcript::Message;

/// What one model call sends of the transcript.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Window {
    /// The messages the call sends, in order.
    pub messages: Vec<Message>,
    /// The most the call may write in the room the window leaves; `None`
    /// when the window sets no limit of its own.
    pub max_output_tokens: Option<u32>,
}

impl Window {
    /// Every one of `messages`, with no output limit: the window of a
    /// conversation that is never fitted.
    #[must_use]
    pub fn whole(messages: &[Message]) -> Self {
        Self {
            messages: messages.to_vec(),
            max_output_tokens: None,
        }
    }
}

/// Whether the transcript was fitted to the next model call's window, `F`
/// being the embedder's failure.
#[derive(Debug)]
pub enum Fitted<F> {
    /// It fits, as this window.
    Ready(Window),
    /// Fitting it failed, and the run with it.
    Failed(F),
    /// The loop was stopped while it was being fitted.
    Stopped,
}
