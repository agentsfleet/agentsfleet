#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afd_wire::report::ResultOutcome;
use afd_wire::tool_trace::ToolCallStatus;
use afr_egress::testing::CountingMint;
use afr_providers::Message;
use afr_tools::catalog::{HTTP_REQUEST, UPDATE_PLAN};
use afr_tools::{Catalog, Tool};
use tokio_util::sync::CancellationToken;

use super::Loop;
use super::finish::DETAIL_STOPPED;
use crate::engine::{AgentEngine, AgentRun, Meter, RunOutput};
use crate::fixture::{Canned, Frames, Script, call, lease, say, unbounded};
use crate::testing::Discard;

/// A loop hosting `tools`, every lease driven by `script`.
pub(super) fn engine(tools: Vec<Box<dyn Tool>>, script: &Script) -> Loop {
    Loop::new(Catalog::new(tools), script.replay())
}

/// Runs `lease` on `engine` until it ends or `stop` is cancelled.
pub(super) async fn drive(
    engine: &Loop,
    lease: &LeasePayload<'_>,
    stop: &CancellationToken,
) -> (RunOutput, Vec<ActivityFrame<'static>>) {
    let frames = Frames::default();
    let sink = frames.sink();
    let run = AgentRun {
        lease,
        memory: afr_memory::Seed::default(),
        executor: None,
        mint: &CountingMint::never(),
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        events: &sink,
        meter: &Meter::default(),
        stop,
    };
    let output = engine.run(run).await.unwrap();
    (output, frames.taken())
}

/// Each completion frame's call id and status, in order.
pub(super) fn completions(frames: &[ActivityFrame<'_>]) -> Vec<(String, ToolCallStatus)> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallCompleted(done) => Some((
                done.call_id.clone().unwrap().into_owned(),
                done.status.unwrap(),
            )),
            _other => None,
        })
        .collect()
}

/// Each start frame's call id, in order.
fn starts(frames: &[ActivityFrame<'_>]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallStarted(started) => {
                Some(started.call_id.clone().unwrap().into_owned())
            }
            _other => None,
        })
        .collect()
}

/// The argument a fetching call names its target by.
const URL_ARG: &str = "url";

#[tokio::test]
async fn test_loop_runs_tool_calls_until_answer() {
    let script = Script::new([
        vec![
            call(
                "p1",
                UPDATE_PLAN.name(),
                serde_json::json!({"operation": "add"}),
            ),
            call(
                "p2",
                UPDATE_PLAN.name(),
                serde_json::json!({"operation": "pow"}),
            ),
        ],
        vec![say("the sum is 4")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);
    let lease = lease(&[UPDATE_PLAN.name()], unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(
        output.result.outcome,
        ResultOutcome::Completed(afd_wire::report::Completed {})
    );
    assert_eq!(output.result.content, "the sum is 4");
    let sent = script.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[0].tools,
        [UPDATE_PLAN.name()],
        "offered exactly the policy's tools"
    );
    assert_eq!(
        sent[0].instructions,
        "## Installed instructions\n\nRead the run."
    );
    assert_eq!(
        sent[0].messages,
        [Message::User("triage the failed run".to_owned())]
    );
    let fed_back: Vec<_> = sent[1].messages[2..].to_vec();
    assert_eq!(
        fed_back,
        [
            Message::ToolResult {
                call_id: "p1".to_owned(),
                output: "4".to_owned()
            },
            Message::ToolResult {
                call_id: "p2".to_owned(),
                output: "4".to_owned()
            },
        ]
    );
    assert_eq!(output.records.len(), 2);
    assert_eq!(output.trace.unwrap().calls.len(), 2);
}

#[tokio::test]
async fn test_loop_emits_one_start_one_end_per_call() {
    let script = Script::new([
        vec![
            call("a", UPDATE_PLAN.name(), serde_json::json!({})),
            call("b", UPDATE_PLAN.name(), serde_json::json!({})),
        ],
        vec![call("c", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(starts(&frames), ["1", "2", "3"]);
    let succeeded = ToolCallStatus::Succeeded;
    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), succeeded),
            ("2".to_owned(), succeeded),
            ("3".to_owned(), succeeded)
        ]
    );
    let trace_ids: Vec<_> = output
        .trace
        .unwrap()
        .calls
        .iter()
        .map(|row| row.call_id.to_string())
        .collect();
    assert_eq!(trace_ids, ["1", "2", "3"]);
    let record_numbers: Vec<_> = output
        .records
        .iter()
        .map(|record| record.call_number)
        .collect();
    assert_eq!(record_numbers, [1, 2, 3]);
}

#[tokio::test(start_paused = true)]
async fn test_run_end_interrupts_open_calls_once() {
    let script = Script::new([vec![
        call("a", UPDATE_PLAN.name(), serde_json::json!({})),
        call(
            "b",
            "http_request",
            serde_json::json!({URL_ARG: "https://example.com"}),
        ),
    ]]);
    let engine = engine(
        vec![
            Canned::boxed(&UPDATE_PLAN, "4"),
            Canned::boxed(&HTTP_REQUEST, ""),
        ],
        &script,
    );
    let lease = lease(&[UPDATE_PLAN.name(), "http_request"], unbounded());
    let stop = CancellationToken::new();
    let stopper = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        stop.cancel();
    };

    let ((output, frames), ()) = tokio::join!(drive(&engine, &lease, &stop), stopper);

    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), ToolCallStatus::Succeeded),
            ("2".to_owned(), ToolCallStatus::Interrupted)
        ]
    );
    let trace = output.trace.unwrap();
    let rows: Vec<_> = trace
        .calls
        .iter()
        .map(|row| (row.call_id.to_string(), row.status))
        .collect();
    assert_eq!(
        rows,
        [
            ("1".to_owned(), ToolCallStatus::Succeeded),
            ("2".to_owned(), ToolCallStatus::Interrupted)
        ]
    );
    assert_eq!(trace.calls[1].arguments[URL_ARG], "https://example.com");
    assert_eq!(
        output.records.len(),
        1,
        "an interrupted call has no output to record"
    );
    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a stopped run is not a completed one");
    };
    assert_eq!(failure.detail, DETAIL_STOPPED);
    assert_eq!(failure.class, None, "the supervisor's cut names the class");
}

#[tokio::test(start_paused = true)]
async fn a_dropped_run_still_closes_its_open_call_once() {
    let script = Script::new([vec![call("a", "http_request", serde_json::json!({}))]]);
    let engine = engine(vec![Canned::boxed(&HTTP_REQUEST, "")], &script);
    let lease = lease(&["http_request"], unbounded());
    let frames = Frames::default();
    let sink = frames.sink();
    let stop = CancellationToken::new();
    let mint = CountingMint::never();
    let meter = Meter::default();
    let run = engine.run(AgentRun {
        lease: &lease,
        memory: afr_memory::Seed::default(),
        executor: None,
        mint: &mint,
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        events: &sink,
        meter: &meter,
        stop: &stop,
    });

    let ended = tokio::time::timeout(Duration::from_secs(1), run).await;

    assert!(ended.is_err(), "the hanging call kept the run going");
    assert_eq!(
        completions(&frames.taken()),
        [("1".to_owned(), ToolCallStatus::Interrupted)]
    );
}
