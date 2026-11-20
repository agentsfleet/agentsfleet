#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::activity::ActivityFrame;
use af_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, DETAIL_FIELD_MAX_BYTES, ToolCallRecord};
use af_wire::tool_trace::ToolCallStatus;
use afr_tools::catalog::UPDATE_PLAN;
use guffbolt::{Call, ToolOutput};

use af_core::test_util::trace::{Capture, CapturedEvent};

use super::test_calls::{RUN_ID, TURN, call, close, planned, ran};
use super::{EVENT_RECORD_DROPPED, Ledger};
use crate::fixture::{Frames, GITHUB_TOKEN, clean_json, scrub};

/// Calls whose records, at the field cap each, pass the event's budget.
const LARGE_CALLS: usize = 20;

#[test]
fn a_run_without_calls_keeps_a_known_trace_and_no_records() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    assert_eq!(
        serde_json::json!(ledger.finish()),
        serde_json::json!([{"calls": [], "omitted_call_count": 0}, []])
    );
}

/// What `records` spend of their event's budget.
fn spent(records: &[ToolCallRecord<'_>]) -> usize {
    records.iter().map(ToolCallRecord::byte_count).sum()
}

#[test]
fn records_past_the_event_budget_are_not_held_and_a_smaller_one_after_still_is() {
    let capture = Capture::install();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    for _ in 0..LARGE_CALLS {
        call(&ledger, "x".repeat(DETAIL_FIELD_MAX_BYTES));
    }
    call(&ledger, "small".to_owned());
    let (_trace, records) = ledger.finish();
    frames.taken();

    let large = records[0].byte_count();
    let fit = DETAIL_EVENT_MAX_BYTES / large;
    assert!(fit < LARGE_CALLS, "the large calls pass the budget");
    assert_eq!(
        records.len(),
        fit + 1,
        "every large one that fits, then the small one"
    );
    let numbers: Vec<u64> = records.iter().map(|record| record.call_number).collect();
    let last = u64::try_from(LARGE_CALLS + 1).unwrap_or(u64::MAX);
    assert_eq!(
        numbers[fit], last,
        "the small call came after the dropped ones"
    );
    assert!(spent(&records) <= DETAIL_EVENT_MAX_BYTES);
    let dropped: Vec<_> = (capture.events().into_iter())
        .filter(|event| event.field("event") == Some(EVENT_RECORD_DROPPED))
        .collect();
    assert_eq!(dropped.len(), LARGE_CALLS - fit, "one per record kept out");
    let warned = |event: &CapturedEvent| {
        event.level == tracing::Level::WARN && event.field("run_id") == Some(RUN_ID)
    };
    assert!(dropped.iter().all(warned), "each drop warns under its run");
    assert_eq!(
        dropped.first().and_then(|event| event.field("call_number")),
        Some((fit + 1).to_string().as_str()),
        "the first drop is the first call past the budget"
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
        let exited = ToolOutput {
            exit_code: Some(2),
            ..ToolOutput::succeeded("ran".to_owned())
        };
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
        let killed = ToolOutput {
            exit_code: Some(137),
            error_code: Some(ToolErrorCode::OutOfMemory.into()),
            ..ToolOutput::succeeded(String::new())
        };
        ran(&ledger, &planned(), Some(killed));
        let exited = ToolOutput {
            exit_code: Some(2),
            ..ToolOutput::succeeded(String::new())
        };
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
