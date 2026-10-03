//! `OpenAI` Responses: the body one turn posts, and how its stream reads.
//!
//! Each turn sends the whole conversation with `store: false`, so nothing of a
//! fleet's run is kept at the provider between turns. `web_search` is sent as
//! `OpenAI`'s hosted tool: it runs there and its findings arrive as answer text.

use afr_tools::Entry;
use afr_tools::catalog::WEB_SEARCH;
use eventsource_stream::Event;
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dialect::{Decode, Dialect};
use crate::error::{Result, raise};
use crate::provider::{Call, Chunk, Message, Request, Usage};

/// The Responses wire.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Responses;

impl Dialect for Responses {
    const NAME: &'static str = "openai";
    const PATH: &'static str = "/v1/responses";

    type Decoder = Decoder;

    fn authorize(&self, builder: RequestBuilder, key: &str) -> RequestBuilder {
        builder.bearer_auth(key)
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
    #[serde(skip_serializing_if = "str::is_empty")]
    instructions: &'a str,
    input: Vec<Item<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    stream: bool,
    store: bool,
}

impl<'a> Body<'a> {
    fn of(request: &Request<'a>) -> Self {
        let functions = request.tools.iter().map(|spec| Tool::Function {
            name: spec.name,
            description: spec.description,
            parameters: spec.parameters,
        });
        let hosted = request.hosted.iter().copied().filter_map(hosted);
        Self {
            model: request.model,
            instructions: request.instructions,
            input: request.messages.iter().flat_map(items).collect(),
            tools: functions.chain(hosted).collect(),
            stream: true,
            store: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    User,
    Assistant,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item<'a> {
    Message {
        role: Role,
        content: &'a str,
    },
    FunctionCall {
        call_id: &'a str,
        name: &'a str,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: &'a str,
        output: &'a str,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Tool<'a> {
    Function {
        name: &'a str,
        description: &'a str,
        parameters: &'a Value,
    },
    #[serde(rename = "web_search")]
    WebSearch,
}

/// The provider's own spec for a hosted tool, when it offers one.
fn hosted<'t>(entry: &'static Entry) -> Option<Tool<'t>> {
    (entry == &WEB_SEARCH).then_some(Tool::WebSearch)
}

/// One message as input items: an assistant turn is its text, when it has
/// any, then one item per call. A call's arguments travel as JSON text.
fn items(message: &Message) -> Vec<Item<'_>> {
    match message {
        Message::User(text) => vec![Item::Message {
            role: Role::User,
            content: text,
        }],
        Message::Assistant { text, calls } => {
            let said = (!text.is_empty()).then_some(Item::Message {
                role: Role::Assistant,
                content: text,
            });
            let called = calls.iter().map(|call| Item::FunctionCall {
                call_id: &call.id,
                name: &call.name,
                arguments: call.arguments.to_string(),
            });
            said.into_iter().chain(called).collect()
        }
        Message::ToolResult { call_id, output } => {
            vec![Item::FunctionCallOutput { call_id, output }]
        }
    }
}

/// Reads one turn's stream.
#[derive(Debug, Default)]
pub(crate) struct Decoder {
    ended: bool,
}

/// One streamed event, narrowed at the parse; every event this wire does
/// not need reads as `Other`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Streamed {
    #[serde(rename = "response.output_text.delta")]
    Text { delta: String },
    #[serde(
        rename = "response.reasoning_summary_text.delta",
        alias = "response.reasoning_text.delta"
    )]
    Reasoning { delta: String },
    #[serde(rename = "response.output_item.done")]
    ItemDone { item: Done },
    #[serde(rename = "response.completed", alias = "response.incomplete")]
    Completed { response: Finished },
    #[serde(rename = "response.failed")]
    Failed { response: Failing },
    #[serde(rename = "error")]
    Error { code: Option<String> },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Done {
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Finished {
    usage: Option<Spent>,
}

/// What the turn spent. Responses counts cached prompt tokens inside
/// `input_tokens`.
#[derive(Debug, Deserialize)]
struct Spent {
    input_tokens: u64,
    output_tokens: u64,
    input_tokens_details: Option<Cached>,
}

#[derive(Debug, Deserialize)]
struct Cached {
    cached_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct Failing {
    error: Option<Reason>,
}

#[derive(Debug, Deserialize)]
struct Reason {
    code: String,
}

/// The reason a failed turn names when the provider names none.
const UNNAMED: &str = "unnamed_error";

impl Decode for Decoder {
    fn event(&mut self, event: &Event, emit: &mut impl FnMut(Chunk)) -> Result<()> {
        match serde_json::from_str(&event.data)? {
            Streamed::Text { delta } => emit(Chunk::answer(delta)),
            Streamed::Reasoning { delta } => emit(Chunk::reasoning(delta)),
            Streamed::ItemDone {
                item:
                    Done::FunctionCall {
                        call_id,
                        name,
                        arguments,
                    },
            } => emit(Chunk::Call(Call::parsed(call_id, name, &arguments))),
            Streamed::Completed { response } => {
                if let Some(spent) = response.usage {
                    emit(Chunk::Usage(Usage {
                        input: spent.input_tokens,
                        cached_input: spent.input_tokens_details.map_or(0, |d| d.cached_tokens),
                        output: spent.output_tokens,
                    }));
                }
                self.ended = true;
            }
            Streamed::Failed { response } => {
                let code = response.error.map(|reason| reason.code);
                return Err(raise::ended(code.as_deref().unwrap_or(UNNAMED)));
            }
            Streamed::Error { code } => {
                return Err(raise::ended(code.as_deref().unwrap_or(UNNAMED)));
            }
            Streamed::ItemDone { .. } | Streamed::Other => {}
        }
        Ok(())
    }

    fn ended(&self) -> bool {
        self.ended
    }
}

#[cfg(test)]
#[path = "openai_responses/tests.rs"]
mod tests;
