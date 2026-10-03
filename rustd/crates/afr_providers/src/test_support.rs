//! What the wire suites share: a turn's request, its body as JSON, and a
//! decoder fed a stream of events.

#![expect(
    clippy::unwrap_used,
    reason = "test support: a fixture that cannot be built is a broken test"
)]

use afr_tools::{Entry, ToolSpec};
use eventsource_stream::Event;
use serde_json::{Value, json};

use crate::dialect::{Decode, Dialect};
use crate::error::Result;
use crate::provider::{Call, Chunk, Message, Request};

/// The model every suite's turn names.
pub(crate) const MODEL: &str = "model-1";
/// The system prompt every suite's turn carries.
pub(crate) const INSTRUCTIONS: &str = "Read the run.";
/// The question the conversation opens with.
pub(crate) const QUESTION: &str = "why did the build fail?";
/// What the model said before its call.
pub(crate) const PREAMBLE: &str = "Checking the log.";
/// The call the conversation's assistant turn made.
pub(crate) const CALL_ID: &str = "call-1";
/// The tool it called.
pub(crate) const TOOL: &str = "calculator";
/// What that call returned.
pub(crate) const OUTPUT: &str = "4";
/// What the loop said after the result.
pub(crate) const FOLLOW_UP: &str = "Answer now.";

/// A conversation with every message kind: a question, a turn that said
/// something and called a tool, the call's result, and a user message after
/// it.
pub(crate) fn conversation() -> Vec<Message> {
    vec![
        Message::User(QUESTION.to_owned()),
        Message::Assistant {
            text: PREAMBLE.to_owned(),
            calls: vec![Call {
                id: CALL_ID.to_owned(),
                name: TOOL.to_owned(),
                arguments: json!({"expression": "2+2"}),
            }],
        },
        Message::ToolResult {
            call_id: CALL_ID.to_owned(),
            output: OUTPUT.to_owned(),
        },
        Message::User(FOLLOW_UP.to_owned()),
    ]
}

/// The calculator's parameters, as the model is offered them.
pub(crate) fn parameters() -> Value {
    json!({"type": "object"})
}

/// A turn continuing `messages`, offering `tools` and `hosted`.
pub(crate) fn request<'a>(
    messages: &'a [Message],
    tools: &'a [ToolSpec<'a>],
    hosted: &'a [&'static Entry],
) -> Request<'a> {
    Request {
        model: MODEL,
        instructions: INSTRUCTIONS,
        messages,
        tools,
        hosted,
    }
}

/// The calculator as a spec over `parameters`.
pub(crate) fn calculator(parameters: &Value) -> ToolSpec<'_> {
    ToolSpec {
        name: TOOL,
        description: TOOL,
        parameters,
    }
}

/// `request`'s body under `dialect`, read back as JSON.
pub(crate) fn body(dialect: &impl Dialect, request: &Request<'_>) -> Value {
    serde_json::from_slice(&dialect.body(request).unwrap()).unwrap()
}

/// An event carrying `data` as its text.
pub(crate) fn event(data: &str) -> Event {
    Event {
        data: data.to_owned(),
        ..Event::default()
    }
}

/// Every chunk `decoder` emits for `events`, in order, or the first failure.
pub(crate) fn decode(
    decoder: &mut impl Decode,
    events: impl IntoIterator<Item = Value>,
) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    for data in events {
        decoder.event(&event(&data.to_string()), &mut |chunk| chunks.push(chunk))?;
    }
    Ok(chunks)
}
