//! Anthropic Messages: the body one turn posts, and how its stream reads.
//!
//! The body borrows the conversation it sends. The system prompt and the
//! conversation's last block each carry a prompt-cache breakpoint, so a loop
//! re-sending its growing conversation pays full price for the new tail only,
//! the caching `ZeroClaw`'s `anthropic.rs` does. `web_search` is sent as
//! Anthropic's own server tool: its calls run there and stream back as answer
//! text, so they never reach the router.

use afr_tools::Entry;
use afr_tools::catalog::WEB_SEARCH;
use reqwest::RequestBuilder;
use serde::Serialize;
use serde_json::Value;

use self::decode::Decoder;
use crate::dialect::Dialect;
use crate::error::Result;
use crate::provider::{Call, Message, Request};

#[path = "anthropic/decode.rs"]
mod decode;

/// The header the key rides in.
const HEADER_KEY: &str = "x-api-key";
/// The header naming the API version this wire speaks.
const HEADER_VERSION: &str = "anthropic-version";
/// The API version this wire speaks.
const API_VERSION: &str = "2023-06-01";
/// The completion tokens a turn may spend, the bound the Zig runner sent.
const MAX_TOKENS: u32 = 8192;
/// Anthropic's server-side web search, as its tool spec names it.
const WEB_SEARCH_TYPE: &str = "web_search_20250305";
/// The only cache lifetime Messages offers without a beta header.
const CACHE_EPHEMERAL: Cache = Cache { kind: "ephemeral" };

/// The Messages wire.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Messages;

impl Dialect for Messages {
    const NAME: &'static str = "anthropic";
    const PATH: &'static str = "/v1/messages";

    type Decoder = Decoder;

    fn authorize(&self, builder: RequestBuilder, key: &str) -> RequestBuilder {
        builder
            .header(HEADER_KEY, key)
            .header(HEADER_VERSION, API_VERSION)
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
    max_tokens: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<Cached<'a>>,
    messages: Vec<Turn<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Tool<'a>>,
    stream: bool,
}

impl<'a> Body<'a> {
    fn of(request: &Request<'a>) -> Self {
        let system = (!request.instructions.is_empty())
            .then(|| {
                Cached::marked(Block::Text {
                    text: request.instructions,
                })
            })
            .into_iter()
            .collect();
        let functions = request.tools.iter().map(|spec| Tool::Function {
            name: spec.name,
            description: spec.description,
            input_schema: spec.parameters,
        });
        let hosted = request.hosted.iter().copied().filter_map(hosted);
        Self {
            model: request.model,
            max_tokens: MAX_TOKENS,
            system,
            messages: turns(request.messages),
            tools: functions.chain(hosted).collect(),
            stream: true,
        }
    }
}

/// A cache breakpoint.
#[derive(Debug, Clone, Copy, Serialize)]
struct Cache {
    #[serde(rename = "type")]
    kind: &'static str,
}

/// One content block, with the cache breakpoint it may carry.
#[derive(Debug, Serialize)]
struct Cached<'a> {
    #[serde(flatten)]
    block: Block<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<Cache>,
}

impl<'a> Cached<'a> {
    const fn plain(block: Block<'a>) -> Self {
        Self {
            block,
            cache_control: None,
        }
    }

    const fn marked(block: Block<'a>) -> Self {
        Self {
            block,
            cache_control: Some(CACHE_EPHEMERAL),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block<'a> {
    Text {
        text: &'a str,
    },
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: &'a Value,
    },
    ToolResult {
        tool_use_id: &'a str,
        content: &'a str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    User,
    Assistant,
}

#[derive(Debug, Serialize)]
struct Turn<'a> {
    role: Role,
    content: Vec<Cached<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Tool<'a> {
    Function {
        name: &'a str,
        description: &'a str,
        input_schema: &'a Value,
    },
    Hosted {
        #[serde(rename = "type")]
        kind: &'static str,
        name: &'static str,
    },
}

/// The provider's own spec for a hosted tool, when it offers one.
fn hosted<'t>(entry: &'static Entry) -> Option<Tool<'t>> {
    (entry == &WEB_SEARCH).then_some(Tool::Hosted {
        kind: WEB_SEARCH_TYPE,
        name: WEB_SEARCH.name(),
    })
}

/// The conversation as alternating turns: Messages wants every tool result
/// in the user turn after the call, and consecutive user content merged. The
/// last block carries the conversation's cache breakpoint.
fn turns(messages: &[Message]) -> Vec<Turn<'_>> {
    let mut turns: Vec<Turn<'_>> = Vec::new();
    for message in messages {
        let (role, blocks) = blocks(message);
        match turns.last_mut() {
            Some(last) if last.role == role => last.content.extend(blocks),
            _ => turns.push(Turn {
                role,
                content: blocks.collect(),
            }),
        }
    }
    if let Some(last) = turns.last_mut().and_then(|turn| turn.content.last_mut()) {
        last.cache_control = Some(CACHE_EPHEMERAL);
    }
    turns
}

/// One message's role and blocks. Empty text sends no block: Messages
/// refuses an empty text block.
fn blocks(message: &Message) -> (Role, impl Iterator<Item = Cached<'_>>) {
    let (role, text, calls, result): (_, &str, &[Call], _) = match message {
        Message::User(text) => (Role::User, text, &[], None),
        Message::Assistant { text, calls } => (Role::Assistant, text, calls, None),
        Message::ToolResult { call_id, output } => (Role::User, "", &[], Some((call_id, output))),
    };
    let text = (!text.is_empty()).then(|| Cached::plain(Block::Text { text }));
    let uses = calls.iter().map(|call| {
        Cached::plain(Block::ToolUse {
            id: &call.id,
            name: &call.name,
            input: &call.arguments,
        })
    });
    let result = result.map(|(call_id, output)| {
        Cached::plain(Block::ToolResult {
            tool_use_id: call_id,
            content: output,
        })
    });
    (role, text.into_iter().chain(uses).chain(result))
}

#[cfg(test)]
#[path = "anthropic/tests.rs"]
mod tests;
