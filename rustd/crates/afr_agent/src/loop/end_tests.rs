//! How a turn ended: a turn cut at the output limit runs none of its calls,
//! and one the lease stopped ends the run.

#![expect(
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use crate::ResultOutcome;
use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Message;
use afr_tools::ToolErrorCode;
use afr_tools::catalog::UPDATE_PLAN;
use tokio_util::sync::CancellationToken;

use super::finish::DETAIL_STOPPED;
use super::tests::{completions, drive, engine};
use super::{EVENT_PROVIDER_FAILED, REASON_STOPPED};
use crate::fixture::{Canned, Script, call, ended, lease, say, unbounded};

/// What the plan tool answers when it runs.
const RAN: &str = "4";
/// What the model says once its cut call was answered.
const RETRIED: &str = "I will ask again with shorter arguments";

/// The tool result the second turn was sent, the cut call's answer.
fn answered(script: &Script) -> String {
    let sent = script.sent();
    match sent.get(1).and_then(|turn| turn.messages.get(2)) {
        Some(Message::ToolResult { output, .. }) => output.clone(),
        other => panic!("the cut call is answered before the next turn: {other:?}"),
    }
}

#[tokio::test]
async fn test_a_cut_turns_calls_are_answered_and_never_run() {
    let script = Script::new([
        vec![
            call(
                "c1",
                UPDATE_PLAN.name(),
                serde_json::json!({"expression": "2+"}),
            ),
            ended(true),
        ],
        vec![say(RETRIED)],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, RAN)], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(output.result.content, RETRIED, "the run went on");
    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Failed)]
    );
    let refusal = answered(&script);
    assert!(
        refusal.starts_with(&format!(
            "[{}] update_plan ",
            ToolErrorCode::OutputLimitReached
        )),
        "{refusal}"
    );
    assert_ne!(refusal, RAN, "the handler never ran");
}

#[tokio::test]
async fn test_a_whole_turns_calls_run() {
    let script = Script::new([
        vec![
            call(
                "c1",
                UPDATE_PLAN.name(),
                serde_json::json!({"expression": "2+2"}),
            ),
            ended(false),
        ],
        vec![say(RETRIED)],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, RAN)], &script);

    let (_output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Succeeded)]
    );
    assert_eq!(answered(&script), RAN);
}

/// A lease stopped before the model answered ends the run as stopped, and the
/// cut turn is logged as the lease's doing rather than a provider failure.
#[tokio::test]
async fn a_lease_stopped_before_the_model_answers_ends_the_run_as_stopped() {
    let capture = Capture::install();
    let script = Script::new([vec![say(RETRIED)]]);
    let engine = engine(Vec::new(), &script);
    let stop = CancellationToken::new();
    stop.cancel();

    let (output, frames) = drive(&engine, &lease(&[], unbounded()), &stop).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a stopped run is not a completed one");
    };
    assert_eq!(failure.detail, DETAIL_STOPPED);
    assert_eq!(output.result.content, "", "the answer was never heard");
    assert!(frames.is_empty(), "{frames:?}");
    let cut = capture.only(EVENT_PROVIDER_FAILED);
    assert_eq!(cut.level, tracing::Level::DEBUG);
    assert_eq!(cut.field("reason"), Some(REASON_STOPPED));
}
