//! What a fake provider on the Messages wire streams back: one turn that
//! answers with text and ends, as `MiniMax` and Anthropic send it.

use serde_json::{Value, json};

/// The server-sent events of one Messages turn in which `model` answers
/// `text`: the message and its one text block opened, the text, both closed.
#[must_use]
pub fn answer(model: &str, text: &str) -> Vec<String> {
    let events: [(&str, Value); 6] = [
        (
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_1", "type": "message",
                "role": "assistant", "model": model, "content": [], "stop_reason": null,
                "stop_sequence": null, "usage": {"input_tokens": 9, "output_tokens": 0}}}),
        ),
        (
            "content_block_start",
            json!({"type": "content_block_start", "index": 0,
                "content_block": {"type": "text", "text": ""}}),
        ),
        (
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": text}}),
        ),
        (
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 0}),
        ),
        (
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn",
                "stop_sequence": null}, "usage": {"output_tokens": 3}}),
        ),
        ("message_stop", json!({"type": "message_stop"})),
    ];
    events
        .iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n\n"))
        .collect()
}
