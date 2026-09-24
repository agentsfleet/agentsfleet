//! What a person reading a thread sees: each message flattened to its shown
//! lines, and the window that keeps the parent and the latest replies.
//!
//! Split from the read itself, which is about pages and the wire; this half is
//! pure and is what the unit tests grade message by message.

use std::collections::VecDeque;

use serde_json::Value;

use super::{MAX_MESSAGES, Message, Posted, Replies};

/// The keys, anywhere inside an attachment or a block, whose string values a
/// person reading the thread sees.
///
/// `title_link` is here because a CI announcement often carries its run URL
/// nowhere else. `fallback` is not: it repeats the text beside it.
const SHOWN_KEYS: [&str; 6] = ["text", "title", "title_link", "pretext", "value", "url"];

/// The parent and the newest replies seen so far.
#[derive(Debug, Default)]
pub(super) struct Window {
    parent: Option<Message>,
    latest: VecDeque<Message>,
    seen: usize,
}

impl Window {
    /// Takes the next message in thread order, dropping the oldest reply once
    /// the window is full.
    pub(super) fn push(&mut self, message: Message) {
        self.seen += 1;
        if self.parent.is_none() {
            self.parent = Some(message);
            return;
        }
        if self.latest.len() == MAX_MESSAGES - 1 {
            self.latest.pop_front();
        }
        self.latest.push_back(message);
    }

    pub(super) fn into_replies(self) -> Replies {
        Replies {
            messages: self.parent.into_iter().chain(self.latest).collect(),
            seen: self.seen,
        }
    }
}

/// One posted message, as the lines a person reading it sees.
pub(super) fn flatten(posted: Posted) -> Message {
    let mut lines = Vec::new();
    keep(&posted.text, &mut lines);
    for part in posted.attachments.iter().chain(&posted.blocks) {
        collect(part, &mut lines);
    }
    Message {
        ts: posted.ts,
        author: posted.user.or(posted.bot_id).unwrap_or_default(),
        text: lines.join("\n"),
    }
}

/// Every shown string inside `value`, in document order, each once.
fn collect(value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                match field {
                    Value::String(text) if SHOWN_KEYS.contains(&key.as_str()) => {
                        keep(text, lines);
                    }
                    _ => collect(field, lines),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, lines)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// Keeps a non-empty line not already kept: Slack repeats a message's text in
/// its blocks, and the fleet needs it once.
fn keep(text: &str, lines: &mut Vec<String>) {
    if !text.is_empty() && !lines.iter().any(|kept| kept == text) {
        lines.push(text.to_owned());
    }
}
