//! A hold as a runner reports it: the wire's `held_until_ms`, through the
//! plane's report and its commit, to the fleet's slot.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use super::slot;
use crate::queue;
use crate::report_commit::{RESPONSE_ACCEPTED, RESUME_EVENT_ID, RESUME_RESPONSE};
use crate::report_seed::{Held, SLICE_MS, held};

/// A processed report on `held`'s lease, asking to hold its sandbox until
/// `held_until_ms`.
fn processed(held: &Held, held_until_ms: Option<i64>) -> ReportRequest<'_> {
    ReportRequest {
        lease_id: Cow::Borrowed(held.issued.lease_id.as_str()),
        event_id: Cow::Borrowed(&held.event_id),
        fencing_token: held.fence.as_u64(),
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE_ACCEPTED),
        tokens: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: SLICE_MS.unsigned_abs(),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(RESUME_EVENT_ID),
            last_response: Cow::Borrowed(RESUME_RESPONSE),
        },
        tool_calls: None,
        held_until_ms,
    }
}

/// A report that left its sandbox held records the hold on the fleet's slot,
/// in the report's own transaction, cut to one idle window from the report.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_carrying_held_until_records_the_hold() {
    let held = held().await;
    let reported_at = held.now.saturating_add_millis(SLICE_MS);
    let window_end = reported_at.saturating_add_millis(SANDBOX_HOLD_IDLE_MS);
    let asked = window_end.saturating_add_millis(SANDBOX_HOLD_IDLE_MS);

    held.fixtures
        .plane()
        .report(
            &held.runner,
            &processed(&held, Some(asked.as_millis())),
            reported_at,
        )
        .await
        .expect("the plane settles the report");

    assert_eq!(
        slot(&held.fixtures, &held.fleet).await,
        (
            Some(window_end.as_millis()),
            Some(held.runner.as_str().to_owned())
        ),
        "the slot holds the fleet for its runner, one idle window and no longer"
    );
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
