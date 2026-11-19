//! The refusal vocabulary a tool reads `agentsfleetd`'s answers through.

use af_core::error_code::{self, ErrorCode};

use super::{CLOSED, LeaseVerbs as _, ScheduleCall, Unanswered, answered};
use crate::runtime::ToolErrorCode;

/// The five codes a model acts on differently each name their own tool error.
#[test]
fn the_codes_a_model_acts_on_each_have_their_own_tool_error() {
    let cases: [(ErrorCode, ToolErrorCode); 5] = [
        (
            error_code::SCHEDULE_CAP_REACHED,
            ToolErrorCode::ScheduleCapReached,
        ),
        (
            error_code::SCHEDULE_NOT_FLEET_OWNED,
            ToolErrorCode::ScheduleNotFleetOwned,
        ),
        (
            error_code::SCHEDULE_NOT_RUNNABLE,
            ToolErrorCode::ScheduleNotRunnable,
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

const ID: &str = "019f0000-0000-7000-8000-000000000001";

fn task_version() -> af_wire::task::DecisionTaskVersion {
    af_wire::task::DecisionTaskVersion {
        task_id: ID.to_owned(),
        generation: 1,
    }
}

/// A verbs implementation that names only the two required verbs refuses every task verb.
#[tokio::test]
async fn the_task_verbs_a_seam_leaves_out_refuse_without_a_code() {
    use af_wire::task::{
        DecisionAssessment, DecisionReservationInput, TaskBindingsInput, TaskEffectInput,
        TaskEffectsQuery, TaskInputMode, TaskInputReceiptInput, TaskInputsQuery,
    };
    let refused = Some(Unanswered::Refused(None));
    let inputs = TaskInputsQuery {
        fencing_token: 1,
        starting_after: None,
        after_input_sequence: None,
        limit: 1,
    };
    assert_eq!(CLOSED.task_inputs(inputs).await.err(), refused);
    let bindings = TaskBindingsInput {
        fencing_token: 1,
        source_admission_id: ID.to_owned(),
        accepted_revision_id: ID.to_owned(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: ID.to_owned(),
        items: Vec::new(),
    };
    assert_eq!(CLOSED.task_bindings(bindings).await.err(), refused);
    let receipt = TaskInputReceiptInput {
        fencing_token: 1,
        operation_id: ID.to_owned(),
        source_admission_id: ID.to_owned(),
        plan_digest: "0".repeat(64),
        item_id: ID.to_owned(),
        task: None,
        mode: TaskInputMode::Queued,
        response_text: None,
    };
    assert_eq!(CLOSED.task_input_receipt(receipt).await.err(), refused);
    let decision = DecisionReservationInput {
        operation_id: ID.to_owned(),
        fencing_token: 1,
        source_admission_id: ID.to_owned(),
        assessment: DecisionAssessment::Message,
        task: Some(task_version()),
        retry_of: None,
        evidence: serde_json::Value::Null,
    };
    assert_eq!(CLOSED.task_decision(decision).await.err(), refused);
    let publish = TaskEffectInput {
        operation_id: ID.to_owned(),
        fencing_token: 1,
        task: task_version(),
        source_item_id: ID.to_owned(),
        covered_input_sequence: 0,
        expected_head: "0".repeat(40),
        tested_tree: "0".repeat(40),
        changes: Vec::new(),
    };
    assert_eq!(CLOSED.task_publish(publish).await.err(), refused);
    assert_eq!(CLOSED.task_effect(ID).await.err(), refused);
    let effects = TaskEffectsQuery {
        fencing_token: 1,
        task_id: ID.to_owned(),
        ordinal_after: 0,
        starting_after: None,
        limit: 1,
    };
    assert_eq!(CLOSED.task_effects(effects).await.err(), refused);
}

/// A decision's debug view says whether a receipt exists without printing it.
#[test]
fn a_decision_evaluation_debug_view_omits_the_recorded_receipt() {
    let evaluation = super::DecisionEvaluation {
        call_id: ID.to_owned(),
        state: af_wire::task::DecisionCallState::Unknown,
        recorded: None,
    };
    assert_eq!(
        format!("{evaluation:?}"),
        format!("DecisionEvaluation {{ call_id: {ID:?}, state: Unknown, recorded: false }}")
    );
}
