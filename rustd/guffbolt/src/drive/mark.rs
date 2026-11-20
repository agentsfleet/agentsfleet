//! Input as the embedder hands it to the loop, and the mark it may carry.
//!
//! A mark is opaque to the loop: the embedder names where a piece of input
//! came from, such as the message it was sent as. The loop compares none and
//! reads none. It keeps the newest mark it has taken in the [`DriveState`],
//! notes that mark as each step begins, and tells the observer the mark with
//! each call's [`crate::CallRef`] when the call ends ([`crate::CallEnd`]). So
//! whatever records a call can say which input was newest when the model
//! asked for it.

use serde::{Deserialize, Serialize};

use super::DriveState;
use crate::conversation::Conversation;

/// What the embedder calls a piece of input, opaque to the loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Mark(String);

impl Mark {
    /// A mark holding `value`, as the embedder spells it.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The mark as the embedder spelled it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One piece of input that arrived since the last model call: the text the
/// model reads, and the mark it carries, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// What the model reads, as it may read it.
    pub text: String,
    /// Where the input came from, as the embedder names it; none for input
    /// the embedder names nothing, such as what a parent sends its child.
    pub mark: Option<Mark>,
}

impl Input {
    /// `text`, marked `mark`.
    #[must_use]
    pub fn marked(text: impl Into<String>, mark: Mark) -> Self {
        Self {
            text: text.into(),
            mark: Some(mark),
        }
    }
}

/// Unmarked input: the text alone.
impl From<String> for Input {
    fn from(text: String) -> Self {
        Self { text, mark: None }
    }
}

/// Joins the input that arrived since the last call onto the transcript, and
/// keeps the newest mark among it in `state`; answers whether any came.
/// Unmarked input leaves the newest mark as it was.
pub(super) async fn take_input<C: Conversation + ?Sized>(
    conversation: &mut C,
    state: &mut DriveState,
) -> bool {
    let input = conversation.take_input().await;
    let took = !input.is_empty();
    let transcript = conversation.transcript();
    for Input { text, mark } in input {
        transcript.join(text);
        state.newest = mark.or_else(|| state.newest.take());
    }
    took
}
