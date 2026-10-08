//! `message`: a line posted, or the reason it was not.

use serde_json::json;

use super::{DELIVERED, Message, NOT_DELIVERED};
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, RecordingVerbs, call, lease_with};
use crate::verbs::Unanswered;

/// Posts `text` through verbs answering `delivered`.
async fn posted(
    delivered: Result<bool, Unanswered>,
    text: &str,
) -> (crate::runtime::ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(String::new()), delivered);
    let lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(Message).as_ref(),
        &lease,
        json!({"text": text}),
    )
    .await;
    (output, verbs.asked())
}

#[tokio::test]
async fn test_message_tool_posts_or_names_gap() {
    let (output, asked) = posted(Ok(true), "fix pushed as a draft").await;
    assert_eq!(output.text, DELIVERED);
    assert_eq!(output.error_code, None);
    assert_eq!(asked, [Asked::Message("fix pushed as a draft".to_owned())]);

    let (output, _asked) = posted(
        Err(Unanswered::Refused(Some(
            afd_core::error_code::MESSAGE_NO_CHANNEL,
        ))),
        "anyone there?",
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::MessageNoChannel));
    assert!(output.text.contains("UZ-RUN-019"), "{}", output.text);
}

/// The thread did not take the line: the model is told, and says it in the
/// answer instead.
#[tokio::test]
async fn an_undelivered_line_is_a_failed_call() {
    let (output, _asked) = posted(Ok(false), "status").await;
    assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamUnreachable));
    assert!(output.text.ends_with(NOT_DELIVERED), "{}", output.text);
}

#[tokio::test]
async fn a_spent_budget_is_its_own_error() {
    let (output, _asked) = posted(
        Err(Unanswered::Refused(Some(
            afd_core::error_code::MESSAGE_LIMIT_REACHED,
        ))),
        "one more",
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::MessageLimitReached));
}
