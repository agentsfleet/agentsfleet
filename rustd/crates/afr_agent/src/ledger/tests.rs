#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::activity::ActivityFrame;
use afd_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, DETAIL_FIELD_MAX_BYTES, ToolCallRecord};
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Call;
use afr_tools::ToolOutput;
use afr_tools::catalog::UPDATE_PLAN;

use afd_core::test_util::trace::{Capture, CapturedEvent};

use super::{EVENT_RECORD_DROPPED, Ledger};
use crate::fixture::{Frames, scrub};

/// The lease every call here belongs to.
const LEASE_ID: &str = "lease-1";
/// Calls whose records, at the field cap each, pass the event's budget.
const LARGE_CALLS: usize = 20;

/// Runs one call through `ledger`, its handler answering `text`.
async fn call(ledger: &Ledger<'_>, text: String) {
    let call = Call {
        id: String::new(),
        name: UPDATE_PLAN.name().to_owned(),
        arguments: serde_json::json!({}),
    };
    ledger
        .call(&call, async { ToolOutput::succeeded(text) })
        .await;
}

/// What `records` spend of their event's budget.
fn spent(records: &[ToolCallRecord<'_>]) -> usize {
    records.iter().map(ToolCallRecord::byte_count).sum()
}

#[tokio::test]
async fn records_past_the_event_budget_are_not_held_and_a_smaller_one_after_still_is() {
    let capture = Capture::install();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(LEASE_ID, &sink, &scrub);

    for _ in 0..LARGE_CALLS {
        call(&ledger, "x".repeat(DETAIL_FIELD_MAX_BYTES)).await;
    }
    call(&ledger, "small".to_owned()).await;
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
        event.level == tracing::Level::WARN && event.field("lease_id") == Some(LEASE_ID)
    };
    assert!(
        dropped.iter().all(warned),
        "each drop warns under its lease"
    );
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
        let ledger = Ledger::new(LEASE_ID, &sink, &scrub);
        call(&ledger, "planned".to_owned()).await;
        let made_up = Call {
            id: String::new(),
            name: "made_up_tool".to_owned(),
            arguments: serde_json::json!({}),
        };
        let exited = ToolOutput {
            exit_code: Some(2),
            ..ToolOutput::succeeded("ran".to_owned())
        };
        ledger.call(&made_up, async { exited }).await;
        let stalled = ledger.call(&made_up, std::future::pending());
        // Polled once and dropped: the run ended before the call did.
        let _interrupted = futures_util::poll!(Box::pin(stalled));
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

/// A call to `UPDATE_PLAN`, which every call here stands in for.
fn planned() -> Call {
    Call {
        id: String::new(),
        name: UPDATE_PLAN.name().to_owned(),
        arguments: serde_json::json!({}),
    }
}

/// The call ids the start frames announced, then how each call ended, in
/// the order the frames left.
fn opened_and_ended(frames: &[ActivityFrame<'_>]) -> (Vec<String>, Vec<(String, ToolCallStatus)>) {
    let mut opened = Vec::new();
    let mut ended = Vec::new();
    for frame in frames {
        match frame {
            ActivityFrame::ToolCallStarted(started) => {
                opened.extend(started.call_id.as_deref().map(str::to_owned));
            }
            ActivityFrame::ToolCallCompleted(done) => {
                ended.extend(done.call_id.as_deref().map(str::to_owned).zip(done.status));
            }
            _ => {}
        }
    }
    (opened, ended)
}

/// A call opened while another call's handler runs, as a child's call opens
/// inside its parent's `delegate`, takes the next number from the one
/// counter, and each ends once, the inner first.
#[tokio::test]
async fn a_call_opened_inside_another_takes_the_next_number_and_each_ends_once() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(LEASE_ID, &sink, &scrub);
    let (outer, inner) = (planned(), planned());

    ledger
        .call(&outer, async {
            ledger
                .call(&inner, async { ToolOutput::succeeded("inner") })
                .await;
            ToolOutput::succeeded("outer")
        })
        .await;
    let (trace, records) = ledger.finish();

    let (opened, ended) = opened_and_ended(&frames.taken());
    assert_eq!(opened, ["1", "2"]);
    let succeeded = ToolCallStatus::Succeeded;
    assert_eq!(
        ended,
        [("2".to_owned(), succeeded), ("1".to_owned(), succeeded)]
    );
    let rows: Vec<&str> = trace
        .iter()
        .flat_map(|trace| &trace.calls)
        .map(|row| row.call_id.as_ref())
        .collect();
    assert_eq!(rows, ["2", "1"], "one row each, in the order they ended");
    assert_eq!(records.len(), 2);
}

/// An outer call whose handler is dropped mid-way, as a parent's run ending
/// drops its `delegate` call and the child inside it, ends both
/// `interrupted`, once each.
#[tokio::test]
async fn dropping_a_call_with_one_open_inside_it_interrupts_both_once() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(LEASE_ID, &sink, &scrub);
    let (outer, inner) = (planned(), planned());

    let running = ledger.call(&outer, async {
        ledger
            .call(&inner, std::future::pending::<ToolOutput>())
            .await;
        ToolOutput::succeeded("never")
    });
    let stopped = tokio::time::timeout(std::time::Duration::ZERO, running).await;
    let (_trace, records) = ledger.finish();

    assert!(stopped.is_err(), "the outer call was still running");
    let (opened, ended) = opened_and_ended(&frames.taken());
    assert_eq!(opened, ["1", "2"]);
    let interrupted = ToolCallStatus::Interrupted;
    assert_eq!(
        ended,
        [("2".to_owned(), interrupted), ("1".to_owned(), interrupted)]
    );
    assert!(records.is_empty(), "an interrupted call posts no record");
}

/// A call the kernel killed for memory is counted once, and its span carries
/// `error.type = out_of_memory`; a call that only exited non-zero is neither
/// counted nor typed.
#[tokio::test]
async fn a_call_killed_for_memory_is_counted_and_typed_on_its_span() {
    use afd_observability::semconv::{ATTR_ERROR_TYPE, OPERATION_EXECUTE_TOOL};
    use afr_telemetry::testing::{Recorded, Tally, scoped};
    use afr_tools::ToolErrorCode;

    let capture = Capture::install();
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let (tally, recorded) = Tally::new();

    scoped(tally, async {
        let ledger = Ledger::new(LEASE_ID, &sink, &scrub);
        let killed = ToolOutput {
            exit_code: Some(137),
            error_code: Some(ToolErrorCode::OutOfMemory),
            ..ToolOutput::succeeded(String::new())
        };
        ledger.call(&planned(), async { killed }).await;
        let exited = ToolOutput {
            exit_code: Some(2),
            ..ToolOutput::succeeded(String::new())
        };
        ledger.call(&planned(), async { exited }).await;
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
