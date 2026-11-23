//! One message of a conversation, the tool calls an answer carries, and the
//! provider's own record of the step that wrote it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Image;

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
