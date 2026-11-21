#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::activity::ActivityFrame;
use af_wire::tool_trace::ToolCallStatus;
use afr_tools::catalog::UPDATE_PLAN;
use guffbolt::{Call, ToolOutput};

use af_core::test_util::trace::Capture;

use super::Ledger;
use super::test_calls::{RUN_ID, TURN, call, close, planned, ran};
use crate::fixture::{Frames, GITHUB_TOKEN, clean_json, scrub};

/// Past the 1 MiB an event's records were once held to: 20 calls of 200 KiB.
const LARGE_CALLS: usize = 20;

/// The output each large call returns.
const LARGE_OUTPUT_BYTES: usize = 200 * 1024;

#[test]
fn a_run_without_calls_keeps_a_known_trace_and_no_records() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    assert_eq!(
        serde_json::json!(ledger.finish()),
        serde_json::json!([{"calls": []}, []])
    );
}

/// No field cap and no event budget: each call's whole output is held for
/// its record, and none is dropped however many large ones the run made.
#[test]
fn a_200_kib_output_is_kept_whole_in_its_item_record() {
    let capture = Capture::install();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let large = "x".repeat(LARGE_OUTPUT_BYTES);

    for _ in 0..LARGE_CALLS {
        call(&ledger, large.clone());
    }
    call(&ledger, "small".to_owned());
    let (trace, records) = ledger.finish();
    frames.taken();

    assert_eq!(trace.calls.len(), LARGE_CALLS + 1);
    assert_eq!(records.len(), LARGE_CALLS + 1, "every record is held");
    assert!(
        records
            .iter()
            .take(LARGE_CALLS)
            .all(|record| record.output == large && !record.truncated),
        "each large output is whole"
    );
    assert_eq!(records[LARGE_CALLS].output, "small");
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.level != tracing::Level::WARN),
        "nothing is dropped"
    );
}

/// Every call's end is counted under its catalog name and its trace row's
/// status: one that succeeded, one that exited non-zero, one whose run ended
/// before it did, and one to a tool the catalog does not publish.
#[tokio::test]
async fn every_call_is_counted_by_tool_and_how_it_ended() {
    use afr_telemetry::labels::{Tool, ToolOutcome};
    use afr_telemetry::testing::{Recorded, Tally, scoped};

    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let (tally, recorded) = Tally::new();

    scoped(tally, async {
        let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
        call(&ledger, "planned".to_owned());
        let made_up = Call {
            id: String::new(),
            name: "made_up_tool".to_owned(),
            arguments: serde_json::json!({}),
        };
        let mut exited = ToolOutput::succeeded("ran".to_owned());
        exited.exit_code = Some(2);
        ran(&ledger, &made_up, Some(exited));
        // The run ended before the call did.
        ran(&ledger, &made_up, None);
    })
    .await;
    frames.taken();

    let ended: Vec<(Tool, ToolOutcome)> = recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            Recorded::ToolCall(tool, outcome, _elapsed) => Some((tool, outcome)),
            _other => None,
        })
        .collect();
    assert_eq!(
        ended,
        vec![
            (Tool::of(UPDATE_PLAN.name()), ToolOutcome::Succeeded),
            (Tool::of("made_up_tool"), ToolOutcome::Failed),
            (Tool::of("made_up_tool"), ToolOutcome::Interrupted),
        ]
    );
    assert_eq!(Tool::of("made_up_tool").as_str(), "_other");
}

/// The call ids the start frames announced, then how each call ended, in
/// the order the frames left.
fn opened_and_ended(frames: &[ActivityFrame<'_>]) -> (Vec<String>, Vec<(String, ToolCallStatus)>) {
    let mut opened = Vec::new();
    let mut ended = Vec::new();
    for frame in frames {
        match frame {
            ActivityFrame::ToolCallStarted(started) => {
                opened.push(started.call_id.to_string());
            }
            ActivityFrame::ToolCallCompleted(done) => {
                ended.push((done.call_id.to_string(), done.status));
            }
            _ => {}
        }
    }
    (opened, ended)
}

/// A call's start frame carries its arguments as JSON, scrubbed and
/// bounded, and arguments that are not an object as `{}`: never an empty
/// string, which is not JSON.
#[test]
fn test_a_calls_start_frame_carries_its_arguments_as_json() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let named = serde_json::json!({"path": "a.md", "token": GITHUB_TOKEN});

    for arguments in [named.clone(), serde_json::json!("not an object")] {
        let call = Call {
            arguments,
            ..planned()
        };
        ran(&ledger, &call, Some(ToolOutput::succeeded("")));
    }

    let shown: Vec<Option<serde_json::Value>> = frames
        .taken()
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallStarted(started) => {
                Some(serde_json::from_str(&started.args_redacted).ok())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            Some(clean_json(named).into_inner()),
            Some(serde_json::json!({}))
        ]
    );
}

/// A call opened while another is open, as a child's call opens inside its
/// parent's `delegate`, takes the next number from the one counter, and each
/// ends once, the inner first.
#[test]
fn a_call_opened_inside_another_takes_the_next_number_and_each_ends_once() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let (outer, inner) = (planned(), planned());

    let outer_open = ledger.open(&outer);
    ran(&ledger, &inner, Some(ToolOutput::succeeded("inner")));
    let answered = Some(ToolOutput::succeeded("outer"));
    close(&ledger, &outer, outer_open, answered);
    let (trace, records) = ledger.finish();

    let (opened, ended) = opened_and_ended(&frames.taken());
    assert_eq!(opened, ["1", "2"]);
    let succeeded = ToolCallStatus::Succeeded;
    assert_eq!(
        ended,
        [("2".to_owned(), succeeded), ("1".to_owned(), succeeded)]
    );
    let rows: Vec<&str> = trace.calls.iter().map(|row| row.call_id.as_ref()).collect();
    assert_eq!(rows, ["2", "1"], "one row each, in the order they ended");
    assert_eq!(records.len(), 2);
}

/// An outer call interrupted with one open inside it, as a parent's run
/// ending drops its `delegate` call and the child inside it, ends both
/// `interrupted`, once each, and posts no record for either.
#[test]
fn interrupting_a_call_with_one_open_inside_it_ends_both_interrupted() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let (outer, inner) = (planned(), planned());

    let outer_open = ledger.open(&outer);
    let unanswered = ran(&ledger, &inner, None);
    close(&ledger, &outer, outer_open, None);
    let (trace, records) = ledger.finish();

    assert_eq!(unanswered, None, "the model reads nothing");
    let (opened, ended) = opened_and_ended(&frames.taken());
    assert_eq!(opened, ["1", "2"]);
    let interrupted = ToolCallStatus::Interrupted;
    assert_eq!(
        ended,
        [("2".to_owned(), interrupted), ("1".to_owned(), interrupted)]
    );
    let rows: Vec<_> = trace.calls.iter().map(|row| row.status).collect();
    assert_eq!(rows, [interrupted, interrupted], "each keeps its trace row");
    assert!(records.is_empty(), "an interrupted call posts no record");
}

/// A call the kernel killed for memory is counted once, and its span carries
/// `error.type = out_of_memory`; a call that only exited non-zero is neither
/// counted nor typed.
#[tokio::test]
async fn a_call_killed_for_memory_is_counted_and_typed_on_its_span() {
    use af_observability::semconv::{ATTR_ERROR_TYPE, OPERATION_EXECUTE_TOOL};
    use afr_telemetry::testing::{Recorded, Tally, scoped};
    use afr_tools::ToolErrorCode;

    let capture = Capture::install();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let (tally, recorded) = Tally::new();

    scoped(tally, async {
        let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
        let mut killed = ToolOutput::succeeded(String::new());
        killed.exit_code = Some(137);
        killed.error_code = Some(ToolErrorCode::OutOfMemory.into());
        ran(&ledger, &planned(), Some(killed));
        let mut exited = ToolOutput::succeeded(String::new());
        exited.exit_code = Some(2);
        ran(&ledger, &planned(), Some(exited));
    })
    .await;
    frames.taken();

    let kills = recorded
        .try_iter()
        .filter(|recorded| *recorded == Recorded::OutOfMemory)
        .count();
    assert_eq!(
        kills, 1,
        "the killed call is counted, the exited one is not"
    );
    let types: Vec<Option<String>> = capture
        .spans()
        .iter()
        .filter(|span| span.name == OPERATION_EXECUTE_TOOL)
        .map(|span| span.field(ATTR_ERROR_TYPE).map(str::to_owned))
        .collect();
    assert_eq!(
        types,
        [Some(ToolErrorCode::OutOfMemory.as_str().to_owned()), None]
    );
}
