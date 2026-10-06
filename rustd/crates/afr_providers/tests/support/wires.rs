//! Each wire as the fake speaks it: a turn that calls a tool, a turn that
//! answers, where a turn posts, and where its next request carries a tool's
//! result.

use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::Reply;

/// The usage every scripted turn reports: a prompt of [`PROMPT_TOKENS`], of
/// which [`CACHED_TOKENS`] were cache reads, each wire spelling the split as
/// its provider does.
pub(crate) const PROMPT_TOKENS: u64 = 10;
pub(crate) const CACHED_TOKENS: u64 = 4;
const COMPLETION_TOKENS: u64 = 3;

/// One of the three wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    Messages,
    Responses,
    Chat,
}

impl Wire {
    /// Every wire, in the order the suites walk them.
    pub(crate) const ALL: [Self; 3] = [Self::Messages, Self::Responses, Self::Chat];

    /// The provider name that selects this wire at the fake.
    pub(crate) fn provider(self) -> String {
        match self {
            Self::Messages => "anthropic".to_owned(),
            Self::Responses => "openai".to_owned(),
            Self::Chat => super::CHAT_PROVIDER.to_owned(),
        }
    }

    /// Where a turn posts.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::Messages => "/v1/messages",
            Self::Responses => "/v1/responses",
            Self::Chat => "/v1/chat/completions",
        }
    }

    /// Whether `headers` carry `key` the way this wire sends it.
    pub(crate) fn carries_key(self, headers: &HeaderMap, key: &str) -> bool {
        let (name, value) = match self {
            Self::Messages => ("x-api-key", key.to_owned()),
            Self::Responses | Self::Chat => ("authorization", format!("Bearer {key}")),
        };
        headers.get(name).is_some_and(|sent| sent == value.as_str())
    }

    /// A turn that calls `name` with `arguments` under call id `id`.
    pub(crate) fn call(self, id: &str, name: &str, arguments: &Value) -> Reply {
        let raw = arguments.to_string();
        Reply::Stream(match self {
            Self::Messages => messages(tool_use(0, id, name, &raw).into(), "tool_use"),
            Self::Responses => {
                let item = |status: &str, arguments: &str| {
                    json!({"type": "function_call", "id": "fc_1", "call_id": id, "name": name,
                        "arguments": arguments, "status": status})
                };
                responses(
                    vec![
                        json!({"type": "response.output_item.added", "output_index": 0,
                            "item": item("in_progress", "")}),
                        json!({"type": "response.function_call_arguments.delta",
                            "item_id": "fc_1", "output_index": 0, "delta": raw}),
                        json!({"type": "response.function_call_arguments.done",
                            "item_id": "fc_1", "output_index": 0, "arguments": raw}),
                        json!({"type": "response.output_item.done", "output_index": 0,
                            "item": item("completed", &raw)}),
                    ],
                    &[item("completed", &raw)],
                )
            }
            Self::Chat => chat(
                &json!({"role": "assistant", "tool_calls": [{"index": 0, "id": id,
                    "type": "function", "function": {"name": name, "arguments": raw}}]}),
                "tool_calls",
            ),
        })
    }

    /// A turn that answers `text`.
    pub(crate) fn answer(self, text: &str) -> Reply {
        Reply::Stream(match self {
            Self::Messages => messages(
                vec![
                    json!({"type": "content_block_start", "index": 0,
                        "content_block": {"type": "text", "text": ""}}),
                    json!({"type": "content_block_delta", "index": 0,
                        "delta": {"type": "text_delta", "text": text}}),
                    json!({"type": "content_block_stop", "index": 0}),
                ],
                "end_turn",
            ),
            Self::Responses => {
                let item = |status: &str, content: Value| {
                    json!({"type": "message", "id": "msg_1", "role": "assistant",
                        "status": status, "content": content})
                };
                let part = json!({"type": "output_text", "text": text, "annotations": []});
                responses(
                    vec![
                        json!({"type": "response.output_item.added", "output_index": 0,
                            "item": item("in_progress", json!([]))}),
                        json!({"type": "response.content_part.added", "item_id": "msg_1",
                            "output_index": 0, "content_index": 0,
                            "part": {"type": "output_text", "text": "", "annotations": []}}),
                        json!({"type": "response.output_text.delta", "item_id": "msg_1",
                            "output_index": 0, "content_index": 0, "delta": text}),
                        json!({"type": "response.output_text.done", "item_id": "msg_1",
                            "output_index": 0, "content_index": 0, "text": text}),
                        json!({"type": "response.output_item.done", "output_index": 0,
                            "item": item("completed", json!([part]))}),
                    ],
                    &[item("completed", json!([part]))],
                )
            }
            Self::Chat => chat(&json!({"role": "assistant", "content": text}), "stop"),
        })
    }

    /// The tool results a request carries back to the model, in order.
    pub(crate) fn results(self, body: &Value) -> Vec<String> {
        let (list, kind_key, kind, output) = match self {
            Self::Messages => ("messages", "type", "tool_result", "content"),
            Self::Responses => ("input", "type", "function_call_output", "output"),
            Self::Chat => ("messages", "role", "tool", "content"),
        };
        let items = body[list].as_array().into_iter().flatten();
        let blocks = items.flat_map(|item| match &item["content"] {
            Value::Array(blocks) if self == Self::Messages => blocks.clone(),
            _ => vec![item.clone()],
        });
        blocks
            .filter(|block| block[kind_key] == kind)
            .filter_map(|block| text(&block[output]))
            .collect()
    }

    /// The images a request carries back inside tool results, as the media
    /// type each wire names them by; chat completions carry none.
    pub(crate) fn images(self, body: &Value) -> Vec<String> {
        let blocks = |value: &Value| value.as_array().cloned().unwrap_or_default();
        match self {
            Self::Messages => blocks(&body["messages"])
                .iter()
                .flat_map(|message| blocks(&message["content"]))
                .filter(|block| block["type"] == "tool_result")
                .flat_map(|block| blocks(&block["content"]))
                .filter(|part| part["type"] == "image")
                .filter_map(|part| part["source"]["media_type"].as_str().map(str::to_owned))
                .collect(),
            Self::Responses => blocks(&body["input"])
                .iter()
                .filter(|item| item["type"] == "function_call_output")
                .flat_map(|item| blocks(&item["output"]))
                .filter(|part| part["type"] == "input_image")
                .filter_map(|part| {
                    part["image_url"]
                        .as_str()
                        .and_then(|url| url.strip_prefix("data:"))
                        .and_then(|rest| rest.split(';').next())
                        .map(str::to_owned)
                })
                .collect(),
            Self::Chat => Vec::new(),
        }
    }

    /// The tool names a request offers, hosted specs by their type.
    pub(crate) fn offered(self, body: &Value) -> Vec<String> {
        let tools = body["tools"].as_array().into_iter().flatten();
        tools
            .filter_map(|tool| {
                let named = match self {
                    Self::Chat => &tool["function"]["name"],
                    Self::Messages | Self::Responses => &tool["name"],
                };
                named
                    .as_str()
                    .or_else(|| tool["type"].as_str())
                    .map(str::to_owned)
            })
            .collect()
    }
}

/// A Messages turn that thinks `thought`, signed `signature`, then calls
/// `name` with `arguments` under call id `id`.
pub(crate) fn thinking_call(
    thought: &str,
    signature: &str,
    id: &str,
    name: &str,
    arguments: &Value,
) -> Reply {
    let thinking = [
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "thinking", "thinking": "", "signature": ""}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "thinking_delta", "thinking": thought}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "signature_delta", "signature": signature}}),
        json!({"type": "content_block_stop", "index": 0}),
    ];
    let call = tool_use(1, id, name, &arguments.to_string());
    Reply::Stream(messages(
        thinking.into_iter().chain(call).collect(),
        "tool_use",
    ))
}

/// A Messages turn that calls `name` with `arguments` under call id `id`,
/// then stops at its output limit.
pub(crate) fn cut_call(id: &str, name: &str, arguments: &Value) -> Reply {
    let call = tool_use(0, id, name, &arguments.to_string());
    Reply::Stream(messages(call.into(), "max_tokens"))
}

/// A Messages turn that calls `name` under call id `id` with the arguments
/// `raw` as written, JSON or not.
pub(crate) fn raw_call(id: &str, name: &str, raw: &str) -> Reply {
    Reply::Stream(messages(tool_use(0, id, name, raw).into(), "tool_use"))
}

/// A Messages tool-use block at `index`: opened, its arguments `raw`, closed.
fn tool_use(index: u64, id: &str, name: &str, raw: &str) -> [Value; 3] {
    [
        json!({"type": "content_block_start", "index": index,
            "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}}),
        json!({"type": "content_block_delta", "index": index,
            "delta": {"type": "input_json_delta", "partial_json": raw}}),
        json!({"type": "content_block_stop", "index": index}),
    ]
}

/// A Messages turn: `blocks` between its start and its end, stopping for
/// `stop_reason`.
fn messages(blocks: Vec<Value>, stop_reason: &str) -> Vec<String> {
    let start = json!({"type": "message_start", "message": {"id": "msg_1", "type": "message",
        "role": "assistant", "model": "model-1", "content": [], "stop_reason": null,
        "stop_sequence": null, "usage": {"input_tokens": PROMPT_TOKENS - CACHED_TOKENS,
            "cache_read_input_tokens": CACHED_TOKENS, "output_tokens": 0}}});
    let delta = json!({"type": "message_delta",
        "delta": {"stop_reason": stop_reason, "stop_sequence": null},
        "usage": {"output_tokens": COMPLETION_TOKENS}});
    let events = std::iter::once(start)
        .chain(blocks)
        .chain([delta, json!({"type": "message_stop"})]);
    events.map(|data| framed(&data)).collect()
}

/// A Responses turn: its creation, `events`, then its completion holding
/// `output`.
fn responses(events: Vec<Value>, output: &[Value]) -> Vec<String> {
    let response = |status: &str, output: &[Value], usage: Value| {
        json!({"id": "resp_1", "object": "response", "created_at": 1, "status": status,
            "model": "model-1", "output": output, "usage": usage})
    };
    let created = json!({"type": "response.created",
        "response": response("in_progress", &[], Value::Null)});
    let usage = json!({"input_tokens": PROMPT_TOKENS, "output_tokens": COMPLETION_TOKENS,
        "total_tokens": PROMPT_TOKENS + COMPLETION_TOKENS,
        "input_tokens_details": {"cached_tokens": CACHED_TOKENS},
        "output_tokens_details": {"reasoning_tokens": 0}});
    let completed = json!({"type": "response.completed",
        "response": response("completed", output, usage)});
    let numbered = std::iter::once(created).chain(events).chain([completed]);
    numbered
        .zip(0_u64..)
        .map(|(mut data, number)| {
            data["sequence_number"] = json!(number);
            framed(&data)
        })
        .collect()
}

/// A chat turn: one `delta`, its `finish_reason`, its usage, and the end line.
fn chat(delta: &Value, finish_reason: &str) -> Vec<String> {
    let chunk = |choices: Value, usage: Value| {
        json!({"id": "chatcmpl-1", "object": "chat.completion.chunk", "created": 1,
            "model": "model-1", "choices": choices, "usage": usage})
    };
    let chunks = [
        chunk(
            json!([{"index": 0, "delta": delta, "finish_reason": null}]),
            Value::Null,
        ),
        chunk(
            json!([{"index": 0, "delta": {}, "finish_reason": finish_reason}]),
            Value::Null,
        ),
        chunk(
            json!([]),
            json!({"prompt_tokens": PROMPT_TOKENS, "completion_tokens": COMPLETION_TOKENS,
                "total_tokens": PROMPT_TOKENS + COMPLETION_TOKENS,
                "prompt_tokens_details": {"cached_tokens": CACHED_TOKENS}}),
        ),
    ];
    let mut events: Vec<String> = chunks.iter().map(framed).collect();
    events.push("data: [DONE]\n\n".to_owned());
    events
}

/// A tool result's text: a string, or the text blocks a wire may send it as.
fn text(output: &Value) -> Option<String> {
    match output {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => {
            let texts = blocks.iter().filter_map(|block| block["text"].as_str());
            Some(texts.collect())
        }
        _ => None,
    }
}

/// One Server-Sent Event carrying `data`, named by its `type` when it has one,
/// as Messages and Responses name theirs.
fn framed(data: &Value) -> String {
    match data["type"].as_str() {
        Some(name) => format!("event: {name}\ndata: {data}\n\n"),
        None => format!("data: {data}\n\n"),
    }
}
