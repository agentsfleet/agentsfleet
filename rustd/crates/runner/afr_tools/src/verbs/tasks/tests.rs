//! The task tools: one item under the person's message, its plan digest the
//! binding's own, the task read back, and each refusal as `[kind] sentence`.

use af_core::error_code;
use af_core::problem::Problem;
use af_wire::decision::NoulCriteria;
use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskBindingsResponse,
    DurableTaskInputAction, DurableTaskItemBinding, DurableTaskRequestedTarget,
    DurableTaskStatement,
};
use guffbolt::ToolOutput;
use serde_json::json;

use super::{TaskInput, TaskStart};
use crate::handler::{Handler, Typed};
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, RecordingVerbs, call, run_with};
use crate::verbs::Unanswered;

/// The person's message the suite's calls name.
const SOURCE: &str = "0190d5c2-7e1a-7000-8000-00000000a0a1";
/// The task the daemon answers with.
const TASK: &str = "0190d5c2-7e1a-7000-8000-00000000b0b2";

/// What a binding answers with.
type Bound = Result<DurableTaskBindingsResponse<'static>, Unanswered>;

/// Calls `handler` with `arguments` through verbs answering a binding `bound`.
async fn called<H: Handler>(
    handler: H,
    bound: Bound,
    arguments: serde_json::Value,
) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(String::new()), Ok(true)).binding(bound);
    let run = run_with(&verbs);
    let output = call(&*Typed::boxed(handler), &run, arguments).await;
    (output, verbs.asked())
}

/// The daemon's answer for one item that `action` bound to [`TASK`].
fn answered(
    action: DurableTaskInputAction,
    replayed: bool,
) -> DurableTaskBindingsResponse<'static> {
    DurableTaskBindingsResponse {
        items: vec![DurableTaskItemBinding {
            item_id: "item".to_owned(),
            durable_task_id: Some(TASK.to_owned()),
            durable_task_generation: Some(1),
            action,
            applies_here: false,
            input_sequence: None,
        }],
        current_durable_task: None,
        execution_deadline: None,
        replayed,
    }
}

/// A start naming PR 88, with `statements`.
fn start(statements: &serde_json::Value) -> serde_json::Value {
    json!({"title": "Fix the null-cart error", "source_turn_id": SOURCE,
        "source_start": 0, "source_end": 38,
        "target": {"kind": "existing_pr", "repository": "acme/shop", "number": 88},
        "statements": statements})
}

/// One statement, as the model writes it.
fn checks_green() -> serde_json::Value {
    json!([{"id": "checks-green", "question": "Do PR 88's required checks pass on its head?",
        "yes": "Every required check passed on the head commit.",
        "no": "A required check failed or has not run."}])
}

/// A start posts one new-work item under the person's message, its plan
/// digest the SHA-256 of `[source_turn_id, items]` the daemon recomputes, and
/// the model reads that the task runs on its own.
#[tokio::test]
async fn a_task_start_posts_one_new_work_item_under_its_digest() {
    let (output, asked) = called(
        TaskStart,
        Ok(answered(DurableTaskInputAction::NewWork, false)),
        start(&checks_green()),
    )
    .await;

    assert_eq!(
        output.text,
        format!(
            "Started task {TASK}. Its first run follows this one, and it is judged done against \
             its statements; this run goes on as before."
        )
    );
    assert_eq!(
        asked,
        [Asked::Bindings(DurableTaskBindingsInput {
            fencing_token: 0,
            source_turn_id: SOURCE.to_owned(),
            plan_digest: "aa9a3453d5892b9e95f9e347cc66709ee86141a83c197237369cd4ae577430f0"
                .to_owned(),
            items: vec![DurableTaskBindingItem::NewWork {
                source_start: 0,
                source_end: 38,
                title: "Fix the null-cart error".to_owned(),
                target: DurableTaskRequestedTarget::ExistingPr {
                    repository: "acme/shop".to_owned(),
                    number: 88,
                },
                statements: vec![DurableTaskStatement {
                    id: "checks-green".to_owned(),
                    question: "Do PR 88's required checks pass on its head?".to_owned(),
                    criteria: NoulCriteria {
                        yes: "Every required check passed on the head commit.".to_owned(),
                        no: "A required check failed or has not run.".to_owned(),
                    },
                }],
            }],
        })]
    );
}

/// A start the daemon refuses — a machine's turn, or the unfinished-task cap
/// reached — reaches the model as its kind and the registry code's sentence.
#[tokio::test]
async fn a_refused_task_start_returns_its_kind_and_sentence() {
    for code in [
        error_code::DURABLE_TASK_INVALID_TARGET,
        error_code::DURABLE_TASK_LIMIT_EXHAUSTED,
    ] {
        let (output, _asked) = called(
            TaskStart,
            Err(Unanswered::Refused(Some(code))),
            start(&checks_green()),
        )
        .await;

        assert_eq!(
            output.text,
            format!(
                "[agentsfleetd_refused] {}: {}",
                code.as_str(),
                Problem::of(code).hint()
            ),
            "{code:?}"
        );
        assert_eq!(output.asks, None);
    }
}

/// A start with no statement, a blank one, two sharing an id, or a negative
/// span breaks a bound the wire declares and never leaves the runner.
#[tokio::test]
async fn a_start_the_wire_bounds_refuse_sends_nothing() {
    let blank = json!([{"id": "a", "question": "  ", "yes": "y", "no": "n"}]);
    let twice = json!([{"id": "a", "question": "q", "yes": "y", "no": "n"},
        {"id": "a", "question": "r", "yes": "y", "no": "n"}]);
    let mut backwards = start(&checks_green());
    if let Some(fields) = backwards.as_object_mut() {
        fields.insert("source_start".to_owned(), json!(-1));
    }
    for arguments in [start(&json!([])), start(&blank), start(&twice), backwards] {
        let (output, asked) = called(
            TaskStart,
            Ok(answered(DurableTaskInputAction::NewWork, false)),
            arguments.clone(),
        )
        .await;

        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{arguments}"
        );
        assert_eq!(asked, [], "{arguments}");
    }
}

/// The same start again reads as the first task, not a second.
#[tokio::test]
async fn a_replayed_start_reads_as_the_same_task() {
    let (output, _asked) = called(
        TaskStart,
        Ok(answered(DurableTaskInputAction::NewWork, true)),
        start(&checks_green()),
    )
    .await;

    assert_eq!(
        output.text,
        format!("This message already started task {TASK}; it is the same task.")
    );
}

/// An input posts one steer item for the named task, and a task id that is
/// not one is refused here.
#[tokio::test]
async fn a_task_input_posts_one_steer_item() {
    let steer = json!({"durable_task_id": TASK, "source_turn_id": SOURCE,
        "source_start": 0, "source_end": 12});
    let (output, asked) = called(
        TaskInput,
        Ok(answered(DurableTaskInputAction::Steer, false)),
        steer,
    )
    .await;

    assert_eq!(
        output.text,
        format!("Handed this message to task {TASK}; it reads it at its next step.")
    );
    let items: Vec<Vec<DurableTaskBindingItem>> = asked
        .iter()
        .filter_map(|asked| match asked {
            Asked::Bindings(input) => Some(input.items.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        items,
        [vec![DurableTaskBindingItem::Steer {
            source_start: 0,
            source_end: 12,
            durable_task_id: TASK.to_owned(),
        }]]
    );
    assert_eq!(asked.len(), 1);

    let (refused, asked) = called(
        TaskInput,
        Ok(answered(DurableTaskInputAction::Steer, false)),
        json!({"durable_task_id": "the-pr-task", "source_turn_id": SOURCE,
            "source_start": 0, "source_end": 12}),
    )
    .await;
    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::InvalidArguments.into())
    );
    assert_eq!(asked, []);
}
