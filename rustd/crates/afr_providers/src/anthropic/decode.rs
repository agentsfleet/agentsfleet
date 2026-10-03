//! How a Messages turn's stream reads: usage from its start and its delta,
//! text and reasoning as they arrive, and each tool call once its input has
//! streamed whole.

use std::collections::BTreeMap;

use eventsource_stream::Event;
use serde::Deserialize;

use crate::dialect::Decode;
use crate::error::{Result, raise};
use crate::provider::{Call, Chunk, Usage};

/// Reads one turn's stream.
#[derive(Debug, Default)]
pub(crate) struct Decoder {
    /// Tool calls still streaming their input, by content index.
    calls: BTreeMap<usize, Partial>,
    ended: bool,
}

/// A tool call whose input is still arriving.
#[derive(Debug)]
struct Partial {
    id: String,
    name: String,
    input: String,
}

/// One streamed event, narrowed at the parse; every event this wire does
/// not need reads as `Other`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Streamed {
    MessageStart {
        message: Started,
    },
    ContentBlockStart {
        index: usize,
        content_block: Opened,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        usage: Finished,
    },
    MessageStop,
    Error {
        error: Failure,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Started {
    usage: Prompt,
}

/// What the prompt took. Messages counts cache reads and writes apart from
/// `input_tokens`, so the prompt is all three.
#[derive(Debug, Deserialize)]
struct Prompt {
    #[serde(rename = "input_tokens")]
    fresh: u64,
    #[serde(rename = "cache_read_input_tokens")]
    read: Option<u64>,
    #[serde(rename = "cache_creation_input_tokens")]
    written: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Opened {
    ToolUse {
        id: String,
        name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Delta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Finished {
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct Failure {
    #[serde(rename = "type")]
    kind: String,
}

impl Decode for Decoder {
    fn event(&mut self, event: &Event, emit: &mut impl FnMut(Chunk)) -> Result<()> {
        match serde_json::from_str(&event.data)? {
            Streamed::MessageStart { message } => {
                let prompt = message.usage;
                let cached = prompt.read.unwrap_or(0);
                let written = prompt.written.unwrap_or(0);
                emit(Chunk::Usage(Usage {
                    input: prompt.fresh + cached + written,
                    cached_input: cached,
                    output: 0,
                }));
            }
            Streamed::ContentBlockStart {
                index,
                content_block: Opened::ToolUse { id, name },
            } => {
                let input = String::new();
                self.calls.insert(index, Partial { id, name, input });
            }
            Streamed::ContentBlockDelta { index, delta } => self.delta(index, delta, emit),
            Streamed::ContentBlockStop { index } => {
                if let Some(call) = self.calls.remove(&index) {
                    emit(Chunk::Call(Call::parsed(call.id, call.name, &call.input)));
                }
            }
            Streamed::MessageDelta { usage } => emit(Chunk::Usage(Usage {
                output: usage.output_tokens,
                ..Usage::default()
            })),
            Streamed::MessageStop => self.ended = true,
            Streamed::Error { error } => return Err(raise::ended(&error.kind)),
            Streamed::ContentBlockStart { .. } | Streamed::Other => {}
        }
        Ok(())
    }

    fn ended(&self) -> bool {
        self.ended
    }
}

impl Decoder {
    fn delta(&mut self, index: usize, delta: Delta, emit: &mut impl FnMut(Chunk)) {
        match delta {
            Delta::Text { text } => emit(Chunk::answer(text)),
            Delta::Thinking { thinking } => emit(Chunk::reasoning(thinking)),
            Delta::InputJson { partial_json } => {
                if let Some(call) = self.calls.get_mut(&index) {
                    call.input.push_str(&partial_json);
                }
            }
            Delta::Other => {}
        }
    }
}
