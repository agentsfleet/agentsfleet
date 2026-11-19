//! The conversation a loop's model calls read: its messages, the tool calls
//! and provider records they carry, and where the latest action group began.
//!
//! The loop keeps it: input joins it before a call, and each step enters it
//! with its calls' results once they ran. The embedder holds it, so its own
//! steps, such as a task's inputs or a saved copy, read and extend the same
//! messages.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool::Image;

/// What joins input onto a user message already waiting.
const INPUT_JOIN: &str = "\n\n";

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// The provider's id for the call, echoed with its result.
    pub id: String,
    /// The tool's name.
    pub name: String,
    /// The arguments, as the model wrote them.
    pub arguments: Value,
}

/// What a provider needs back on the next step to continue its own reasoning:
/// thinking blocks with their signatures, a reasoning trace, the identity of
/// an output item.
///
/// The provider's own record, opaque to the loop, which keeps it with the
/// step that produced it and hands it back unopened. It serializes as the
/// record itself, so a saved conversation holds exactly what the provider
/// wrote. A step no provider recorded holds none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Replay(Value);

impl Replay {
    /// The provider's record.
    #[must_use]
    pub const fn new(record: Value) -> Self {
        Self(record)
    }

    /// The record, for the provider that wrote it to read back.
    #[must_use]
    pub const fn record(&self) -> &Value {
        &self.0
    }

    /// Whether no provider recorded anything to hand back.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_null()
    }
}

/// One message of the conversation a step continues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// What the model was asked.
    User(String),
    /// What the model answered and which tools it called.
    Assistant {
        /// The answer text of the step, possibly empty.
        text: String,
        /// The calls the step asked for.
        calls: Vec<Call>,
        /// What the provider needs back to continue its own reasoning.
        replay: Replay,
    },
    /// One call's output, fed back to the model.
    ToolResult {
        /// The provider's id for the call.
        call_id: String,
        /// What the call returned.
        output: String,
        /// An image the call read, for the model to see with the output.
        image: Option<Image>,
    },
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
}

#[cfg(test)]
mod tests;
