#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, DETAIL_FIELD_MAX_BYTES, ToolCallRecord};
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
async fn call(ledger: &mut Ledger<'_>, text: String) {
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
    let mut ledger = Ledger::new(LEASE_ID, &sink, &scrub);

    for _ in 0..LARGE_CALLS {
        call(&mut ledger, "x".repeat(DETAIL_FIELD_MAX_BYTES)).await;
    }
    call(&mut ledger, "small".to_owned()).await;
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
        let mut ledger = Ledger::new(LEASE_ID, &sink, &scrub);
        call(&mut ledger, "planned".to_owned()).await;
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
