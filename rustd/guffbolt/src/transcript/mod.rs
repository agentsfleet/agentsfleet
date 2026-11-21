//! The conversation a loop's model calls read: its messages, the tool calls
//! and provider records they carry, and where the latest action group began.
//!
//! The loop keeps it: input joins it before a call, and each step enters it
//! with its calls' results once they ran. The embedder holds it, so its own
//! steps, such as a task's inputs or a saved copy, read and extend the same
//! messages.

use guffbolt_ai::Message;
use serde::{Deserialize, Serialize};

/// What joins input onto a user message already waiting.
const INPUT_JOIN: &str = "\n\n";

/// Where one tool call sits in its conversation.
///
/// The loop hands it to [`crate::Conversation::dispatch`] with the call, so
/// a tool reads it from what its call is lent, and a backend that acts
/// outside the process derives its idempotency key from it. The same place
/// in the conversation always yields the same reference. A tool that retries
/// a call keeps it, and so does a journalled call an embedder sends again on
/// resume. A resumed loop that asks the model again makes new calls, with new
/// references.
///
/// It names one call within one conversation. An embedder whose run holds
/// several loops, such as a root and its children, adds what names the loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallRef {
    /// The step that asked for the call, from 1. It counts every step the
    /// conversation's [`crate::DriveState`] completed, so a resumed loop goes
    /// on from the saved steps, while the step number its observer hears
    /// starts at 1 again.
    pub step: u64,
    /// The call's place among its step's calls, from 0, in the order the
    /// model asked for them.
    pub index: u64,
    /// The provider's id for the call, as [`crate::Call::id`] carries it.
    pub call_id: String,
}

/// One loop's conversation, in order, and where the action group its latest
/// step started begins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    messages: Vec<Message>,
    /// The index of the first message of the current group.
    group: usize,
}

impl Transcript {
    /// A conversation opening with `messages`; the first step's group begins
    /// after them.
    #[must_use]
    pub fn new(messages: Vec<Message>) -> Self {
        let group = messages.len();
        Self { messages, group }
    }

    /// A conversation resuming `messages`, all of them the current group's,
    /// so whatever judges that group first reads the whole resumed history.
    #[must_use]
    pub const fn resumed(messages: Vec<Message>) -> Self {
        Self { messages, group: 0 }
    }

    /// Every message, in order.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// The current group's messages: what its step said and called, its
    /// calls' results, and whatever joined the conversation since.
    #[must_use]
    pub fn group(&self) -> &[Message] {
        self.messages.get(self.group..).unwrap_or_default()
    }

    /// Adds `message` after the others.
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Adds input `text`, joined onto a user message already waiting, so no
    /// provider sees two in a row.
    pub fn join(&mut self, text: String) {
        if let Some(Message::User(waiting)) = self.messages.last_mut() {
            waiting.push_str(INPUT_JOIN);
            waiting.push_str(&text);
        } else {
            self.messages.push(Message::User(text));
        }
    }

    /// Marks where the group the next step starts begins.
    pub(crate) const fn begin_group(&mut self) {
        self.group = self.messages.len();
    }

    /// Adds `messages` after the others.
    pub(crate) fn extend(&mut self, messages: Vec<Message>) {
        self.messages.extend(messages);
    }

    /// Every message, in order, as an owned list.
    pub(crate) fn into_messages(self) -> Vec<Message> {
        self.messages
    }
}

#[cfg(test)]
mod tests;
