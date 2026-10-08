//! An event's message: what a person or a producer asked, as the model reads
//! it. One reading, so the daemon's earlier turns and the runner's current
//! message never disagree about the same request.

use std::borrow::Cow;

/// The request field holding the event's message.
pub const MESSAGE: &str = "message";

/// The event's message: the request's `message` field when it carries one as
/// a string, and the whole request otherwise, the fallback
/// `src/runner/child_exec_input.zig` defines.
#[must_use]
pub fn message_of(request_json: &str) -> Cow<'_, str> {
    serde_json::from_str::<serde_json::Value>(request_json)
        .ok()
        .and_then(|value| {
            value
                .get(MESSAGE)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .map_or(Cow::Borrowed(request_json), Cow::Owned)
}

#[cfg(test)]
mod tests {
    use super::message_of;

    #[test]
    fn a_request_with_a_message_reads_as_that_message() {
        assert_eq!(
            message_of(r#"{"message":"fix the second one"}"#),
            "fix the second one"
        );
    }

    #[test]
    fn a_request_without_a_string_message_reads_as_itself() {
        for request in [r#"{"ref":"main"}"#, r#"{"message":7}"#, "not json"] {
            assert_eq!(message_of(request), request);
        }
    }
}
