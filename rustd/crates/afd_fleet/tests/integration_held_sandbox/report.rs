//! A hold as a runner reports it: the wire's `held_until`, through the
//! plane's report and its commit, to the fleet's slot, capped there at one
//! idle window from the database's clock.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::timing::{LEASE_TTL, SANDBOX_HOLD_IDLE};
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use jiff::Timestamp;

use super::{id, slot};
use crate::report_commit::{RESPONSE_ACCEPTED, RESUME_EVENT_ID, RESUME_RESPONSE};
use crate::report_seed::{Held, SLICE, SLICE_MS, held};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// A processed report on `held`'s lease, asking to hold its sandbox until
/// `held_until`.
pub(crate) fn processed(held: &Held, held_until: Option<Timestamp>) -> ReportRequest<'_> {
    ReportRequest {
        lease_id: Cow::Borrowed(held.issued.lease_id.as_str()),
        event_id: Cow::Borrowed(&held.event_id),
        fencing_token: held.fence.as_u64(),
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE_ACCEPTED),
        tokens: 0,
        requests: afd_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: SLICE_MS.unsigned_abs(),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(RESUME_EVENT_ID),
            last_response: Cow::Borrowed(RESUME_RESPONSE),
        },
        tool_calls: None,
        held_until,
    }
}

/// Runs `write`, answering its output and the window a hold it stores may
/// last: one idle window past the database clock before it, to one past the
/// clock after it.
async fn window_around<T>(
    fixtures: &Fixtures,
    write: impl std::future::Future<Output = T>,
) -> (T, std::ops::RangeInclusive<Timestamp>) {
    let (output, clock) = fixtures.clock_around(write).await;
    let idle = |at: &Timestamp| afd_core::clock::after(*at, SANDBOX_HOLD_IDLE);
    (output, idle(clock.start())..=idle(clock.end()))
}

/// `runner` claims `fleet` and releases it asking to hold its sandbox until
/// `asked`; answers the hold the slot then stores.
async fn released(
    fixtures: &Fixtures,
    fleet: &str,
    runner: &afd_core::id::Uuid7,
    asked: Option<Timestamp>,
) -> Option<Timestamp> {
    let leases = fixtures.leases();
    let claimed = leases
        .claim(&id(fleet), runner, LEASE_TTL)
        .await
        .expect("the claim answers")
        .expect("the runner wins its own fleet");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(&mut connection, &id(fleet), claimed.fence, asked)
        .await
        .expect("the release lands");
    drop(connection);
    slot(fixtures, fleet).await.0
}

/// A report that left its sandbox held records the hold on the fleet's slot,
/// in the report's own transaction, cut to one idle window from the commit.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_carrying_held_until_records_the_hold() {
    let held = held().await;
    let reported_at = afd_core::clock::after(held.now, SLICE);
    let asked = afd_core::clock::after(reported_at, SANDBOX_HOLD_IDLE.saturating_mul(3));
    let plane = held.fixtures.plane();
    let request = processed(&held, Some(asked));

    let (reported, window) = window_around(
        &held.fixtures,
        plane.report(&held.runner, &request, reported_at),
    )
    .await;
    reported.expect("the plane settles the report");

    let (stored, holder) = slot(&held.fixtures, &held.fleet).await;
    let stored = stored.expect("the report recorded a hold");
    assert!(
        window.contains(&stored),
        "the slot holds the fleet one idle window and no longer: {stored} outside {window:?}"
    );
    assert_eq!(holder, Some(held.runner.as_str().to_owned()));
    held.fixtures.cleanup().await;
}

/// What the release stores for each hold a runner may ask: an ask inside the
/// window is kept as asked, one past it is cut to the window's end, and one
/// already behind the database clock, or none at all, holds nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_stored_hold_is_capped_at_one_idle_window() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;

    let inside = afd_core::clock::after(fixtures.database_now().await, SANDBOX_HOLD_IDLE / 2);
    assert_eq!(
        released(&fixtures, &fleet, &runner, Some(inside)).await,
        Some(inside),
        "an ask inside the window is kept as asked"
    );
    for asked in [
        afd_core::clock::after(inside, SANDBOX_HOLD_IDLE),
        Timestamp::MAX,
    ] {
        let (stored, window) =
            window_around(&fixtures, released(&fixtures, &fleet, &runner, Some(asked))).await;
        let stored = stored.expect("a far ask still holds");
        assert!(
            window.contains(&stored),
            "asked {asked}, stored {stored}: past one idle window"
        );
    }
    let behind = afd_core::clock::before(fixtures.database_now().await, SLICE);
    assert_eq!(
        released(&fixtures, &fleet, &runner, Some(behind)).await,
        None,
        "an ask already behind the clock holds nothing"
    );
    assert_eq!(
        released(&fixtures, &fleet, &runner, None).await,
        None,
        "no ask holds nothing"
    );
    fixtures.cleanup().await;
}
