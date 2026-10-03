//! The prompt: the installed instructions as the system prompt, and the
//! event's message as the first user turn.
//!
//! The message is the event's `message` field when the request carries one as
//! a string, and the whole request otherwise, the fallback
//! `src/runner/child_exec_input.zig` defines.

use afd_wire::lease::LeasePayload;

/// The heading the installed instructions render under.
const INSTALLED_INSTRUCTIONS: &str = "## Installed instructions\n\n";
/// The request field holding the event's message.
const FIELD_MESSAGE: &str = "message";

/// What a run asks the model first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prompt {
    /// The system prompt.
    pub(crate) instructions: String,
    /// The first user turn.
    pub(crate) message: String,
}

impl Prompt {
    /// The prompt for `lease`.
    pub(crate) fn new(lease: &LeasePayload<'_>) -> Self {
        let request = lease.event.request_json.as_ref();
        let message = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|value| {
                value
                    .get(FIELD_MESSAGE)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| request.to_owned());
        let instructions = if lease.instructions.is_empty() {
            String::new()
        } else {
            format!("{INSTALLED_INSTRUCTIONS}{}", lease.instructions)
        };
        Self {
            instructions,
            message,
        }
    }
}

#[cfg(test)]
#[path = "prompt/tests.rs"]
mod tests;
