//! The refusal vocabulary a tool reads `agentsfleetd`'s answers through.

use afd_core::error_code::{self, ErrorCode};

use super::{CLOSED, LeaseVerbs as _, ScheduleCall, Unanswered, answered};
use crate::runtime::ToolErrorCode;

/// The four codes a model acts on differently each name their own tool error.
#[test]
fn the_codes_a_model_acts_on_each_have_their_own_tool_error() {
    let cases: [(ErrorCode, ToolErrorCode); 4] = [
        (
            error_code::SCHEDULE_CAP_REACHED,
            ToolErrorCode::ScheduleCapReached,
        ),
        (
            error_code::SCHEDULE_NOT_FLEET_OWNED,
            ToolErrorCode::ScheduleNotFleetOwned,
        ),
        (
            error_code::MESSAGE_NO_CHANNEL,
            ToolErrorCode::MessageNoChannel,
        ),
        (
            error_code::MESSAGE_LIMIT_REACHED,
            ToolErrorCode::MessageLimitReached,
        ),
    ];
    for (code, tool) in cases {
        assert_eq!(
            Unanswered::Refused(Some(code)).tool_code(),
            tool,
            "{code:?}"
        );
    }
}

#[test]
fn any_other_refusal_reads_as_refused_and_names_its_code() {
    let refused = Unanswered::Refused(Some(error_code::SCHEDULE_NOT_FOUND));
    assert_eq!(refused.tool_code(), ToolErrorCode::AgentsfleetdRefused);
    assert!(
        refused
            .detail()
            .starts_with(error_code::SCHEDULE_NOT_FOUND.as_str()),
        "{}",
        refused.detail()
    );
    let unnamed = Unanswered::Refused(None);
    assert_eq!(unnamed.tool_code(), ToolErrorCode::AgentsfleetdRefused);
    assert_eq!(unnamed.detail(), super::DETAIL_REFUSED);
}

#[test]
fn an_unreachable_agentsfleetd_says_so() {
    assert_eq!(
        Unanswered::Unreachable.tool_code(),
        ToolErrorCode::AgentsfleetdUnreachable
    );
    assert_eq!(Unanswered::Unreachable.detail(), super::DETAIL_UNREACHABLE);
}

#[test]
fn a_success_is_the_body_and_an_empty_one_reads_done() {
    let body = answered(Ok(r#"{"schedules":[]}"#.to_owned()));
    assert_eq!(body.text, r#"{"schedules":[]}"#);
    assert_eq!(body.error_code, None);
    assert_eq!(answered(Ok(String::new())).text, super::DONE);
}

#[test]
fn a_refusal_is_the_tool_error_with_its_detail() {
    let refused = answered(Err(Unanswered::Refused(Some(
        error_code::MESSAGE_NO_CHANNEL,
    ))));
    assert_eq!(refused.error_code, Some(ToolErrorCode::MessageNoChannel));
    assert!(
        refused.text.starts_with("[message_no_channel] UZ-RUN-019"),
        "{}",
        refused.text
    );
}

/// A lease with nothing behind it says so for every verb.
#[tokio::test]
async fn closed_verbs_answer_nothing() {
    assert_eq!(
        CLOSED.schedules(ScheduleCall::List).await,
        Err(Unanswered::Unreachable)
    );
    assert_eq!(CLOSED.message("hello").await, Err(Unanswered::Unreachable));
}
