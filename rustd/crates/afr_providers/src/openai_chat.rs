//! `OpenAI`-compatible chat completions: the body one turn posts, and how its
//! stream reads.
//!
//! The wire a `custom:<url>` provider speaks. It has no hosted tools, so no
//! hosted spec is sent: a `web_search` call the model makes anyway reaches the
//! router, which answers it with `hosted_tool_unavailable`. A key is sent only
//! when there is one, since a self-hosted endpoint may take none.

use eventsource_stream::Event;
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dialect::{Decode, Dialect};
use crate::error::Result;
use crate::provider::{Call, Chunk, Message, Request, Usage};

/// The data line that ends a chat stream.
const DONE: &str = "[DONE]";

/// The chat wire.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Chat;

impl Dialect for Chat {
    const NAME: &'static str = "custom";
    const PATH: &'static str = "/chat/completions";

    type Decoder = Decoder;

    fn authorize(&self, builder: RequestBuilder, key: &str) -> RequestBuilder {
        if key.is_empty() {
            builder
        } else {
            builder.bearer_auth(key)
        }
    }

    fn body(&self, request: &Request<'_>) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&Body::of(request))?)
    }

    fn decoder(&self) -> Decoder {
        Decoder::default()
    }
}

/// One turn's request.
#[derive(Debug, Serialize)]
struct Body<'a> {
    model: &'a str,
    messages: Vec<Said<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    stream: bool,
    stream_options: StreamOptions,
}

impl<'a> Body<'a> {
    fn of(request: &Request<'a>) -> Self {
        let system = (!request.instructions.is_empty()).then_some(Said::System {
            content: request.instructions,
        });
        let conversation = request.messages.iter().map(said);
        let tools = request.tools.iter().map(|spec| Tool {
            kind: FUNCTION,
            function: Function {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            },
        });
        Self {
            model: request.model,
            messages: system.into_iter().chain(conversation).collect(),
            tools: tools.collect(),
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
        }
    }
}

/// The type every chat tool and tool call carries.
const FUNCTION: &str = "function";

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum Said<'a> {
    System {
        content: &'a str,
    },
    User {
        content: &'a str,
    },
    Assistant {
        content: Option<&'a str>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<Called<'a>>,
    },
    Tool {
        tool_call_id: &'a str,
        content: &'a str,
    },
}

#[derive(Debug, Serialize)]
struct Tool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: Function<'a>,
}

#[derive(Debug, Serialize)]
struct Function<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Debug, Serialize)]
struct Called<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: Invoked<'a>,
}

#[derive(Debug, Serialize)]
struct Invoked<'a> {
    name: &'a str,
    arguments: String,
}

/// One message as chat says it. An assistant turn with no text sends `null`
/// content, as the wire asks of one that only calls tools.
fn said(message: &Message) -> Said<'_> {
    match message {
        Message::User(text) => Said::User { content: text },
        Message::Assistant { text, calls } => Said::Assistant {
            content: (!text.is_empty()).then_some(text.as_str()),
            tool_calls: calls
                .iter()
                .map(|call| Called {
                    id: &call.id,
                    kind: FUNCTION,
                    function: Invoked {
                        name: &call.name,
                        arguments: call.arguments.to_string(),
                    },
                })
                .collect(),
        },
        Message::ToolResult { call_id, output } => Said::Tool {
            tool_call_id: call_id,
            content: output,
        },
    }
}

/// Reads one turn's stream.
#[derive(Debug, Default)]
pub(crate) struct Decoder {
    /// Tool calls still streaming, by the index the wire gives each.
    calls: Vec<Partial>,
    /// A choice finished: the turn has its answer and its calls.
    finished: bool,
}

/// A tool call whose pieces are still arriving.
#[derive(Debug, Default)]
struct Partial {
    id: String,
    name: String,
    arguments: String,
}

/// One streamed chunk, narrowed at the parse.
#[derive(Debug, Deserialize)]
struct Streamed {
    #[serde(default)]
    choices: Vec<Choice>,
    usage: Option<Spent>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Delta {
    content: Option<String>,
    #[serde(alias = "reasoning")]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<Piece>,
}

/// One piece of a tool call: its id and name arrive once, its arguments in
/// fragments.
#[derive(Debug, Deserialize)]
struct Piece {
    index: usize,
    id: Option<String>,
    function: Option<Fragment>,
}

#[derive(Debug, Deserialize)]
struct Fragment {
    name: Option<String>,
    arguments: Option<String>,
}

/// What the turn spent. Chat counts cached prompt tokens inside
/// `prompt_tokens`.
#[derive(Debug, Deserialize)]
struct Spent {
    prompt_tokens: u64,
    completion_tokens: u64,
    prompt_tokens_details: Option<Cached>,
}

#[derive(Debug, Deserialize)]
struct Cached {
    cached_tokens: u64,
}

impl Decode for Decoder {
    fn event(&mut self, event: &Event, emit: &mut impl FnMut(Chunk)) -> Result<()> {
        if event.data.trim() == DONE {
            self.finish(emit);
            return Ok(());
        }
        let streamed: Streamed = serde_json::from_str(&event.data)?;
        for choice in streamed.choices {
            self.choice(choice, emit);
        }
        if let Some(spent) = streamed.usage {
            emit(Chunk::Usage(Usage {
                input: spent.prompt_tokens,
                cached_input: spent.prompt_tokens_details.map_or(0, |d| d.cached_tokens),
                output: spent.completion_tokens,
            }));
        }
        Ok(())
    }

    fn ended(&self) -> bool {
        self.finished
    }
}

impl Decoder {
    fn choice(&mut self, choice: Choice, emit: &mut impl FnMut(Chunk)) {
        let delta = choice.delta;
        if let Some(reasoning) = delta.reasoning_content.filter(|text| !text.is_empty()) {
            emit(Chunk::reasoning(reasoning));
        }
        if let Some(content) = delta.content.filter(|text| !text.is_empty()) {
            emit(Chunk::answer(content));
        }
        for piece in delta.tool_calls {
            self.piece(piece);
        }
        if choice.finish_reason.is_some() {
            self.finish(emit);
        }
    }

    fn piece(&mut self, piece: Piece) {
        if self.calls.len() <= piece.index {
            self.calls.resize_with(piece.index + 1, Partial::default);
        }
        let Some(call) = self.calls.get_mut(piece.index) else {
            return;
        };
        if let Some(id) = piece.id {
            call.id = id;
        }
        let fragment = piece.function.unwrap_or(Fragment {
            name: None,
            arguments: None,
        });
        if let Some(name) = fragment.name {
            call.name = name;
        }
        if let Some(arguments) = fragment.arguments {
            call.arguments.push_str(&arguments);
        }
    }

    /// Ends the turn: every assembled call goes out, once.
    fn finish(&mut self, emit: &mut impl FnMut(Chunk)) {
        self.finished = true;
        for call in self.calls.drain(..).filter(|call| !call.name.is_empty()) {
            emit(Chunk::Call(Call::parsed(
                call.id,
                call.name,
                &call.arguments,
            )));
        }
    }
}

#[cfg(test)]
#[path = "openai_chat/tests.rs"]
mod tests;
