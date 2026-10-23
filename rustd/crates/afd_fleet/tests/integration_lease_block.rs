//! §2's refusal write against a live Postgres: ending an event at a gate.
//!
//! The statement's whole safety property is a `WHERE … AND status = $6`, and a
//! guard is exactly the thing that cannot be proven by reading it. What is
//! tested here is that a terminal row is never reopened, because the failure
//! that guard prevents is silent: a refused event resurrected by a redelivery
//! would run work a gate already denied, under a label nobody would go back
//! and re-read.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::seed;
use crate::support;
use afd_fleet::lease::admit::Refusal;
use afd_fleet::lease::{Delivery, Ended};
use sqlx::Row as _;

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// The label a first refusal writes.
const FIRST_LABEL: &str = "balance_exhausted";

/// A different label, so a reopened row is distinguishable from a held one.
const SECOND_LABEL: &str = "approval_denied";

/// An operator-readable instruction, to prove it lands beside the label.
const DETAIL: &str = "top up the workspace balance";

/// Two columns of one `fleet.events` row, as text.
async fn failure_of(fixtures: &Fixtures, fleet: &str, event: &str) -> (String, Option<String>) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT status, failure_reason, failure_detail
           FROM fleet.events WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(fleet)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("the event row must be readable");

    let status: String = row.try_get(0).expect("status is text");
    let label: String = row.try_get(1).expect("failure_reason is text");
    let detail: Option<String> = row.try_get(2).expect("failure_detail is nullable text");
    assert_eq!(status, "gate_blocked", "the row must be terminal");
    (label, detail)
}

/// A leased event with its narrative log open, ready to be refused.
async fn received(fixtures: &Fixtures) -> afd_fleet::lease::Acquired {
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(fixtures).await;
    let leases = fixtures.leases();
    let held = crate::seed::select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the fleet is leasable");
    assert_eq!(
        leases
            .record_received(&held)
            .await
            .expect("the narrative log must open")
            .delivery,
        Delivery::First
    );
    held
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_refusal_ends_the_event_and_names_what_refused_it() {
    let fixtures = Fixtures::create_with_queue().await;
    let acquired = received(&fixtures).await;
    let fleet = &acquired.fleet_id;
    let event = &acquired.event_id;

    let ended = fixtures
        .leases()
        .block(
            &acquired,
            Refusal {
                label: FIRST_LABEL,
                detail: DETAIL,
            },
        )
        .await
        .expect("the refusal must write");

    assert!(
        matches!(ended, Ended::Now(_)),
        "the first refusal decides, and carries the closing it wrote"
    );
    // The closing reads the counters beside the row it ends, so the frame it
    // announces carries the figures the database holds at that instant.
    // The two counters differ (one event, nothing charged), so a closing that
    // swapped their positions would disagree with the read below.
    if let Ended::Now(closed) = &ended {
        let stored = afd_events::fleet_counters(&fixtures.database, fleet.as_str())
            .await
            .expect("the counters read back");
        assert_ne!(stored.events_processed, stored.budget_used_nanos);
        assert_eq!(
            closed.counters, stored,
            "the closing's snapshot is the database's"
        );
    }
    let (label, detail) = failure_of(&fixtures, fleet.as_str(), event).await;
    assert_eq!(label, FIRST_LABEL);
    assert_eq!(detail.as_deref(), Some(DETAIL));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_terminal_row_is_never_reopened_by_a_second_refusal() {
    // The guard, and the reason it exists. A refused delivery whose
    // acknowledgement was lost comes back; without the `status = 'received'`
    // predicate the second pass would rewrite the row under a different label,
    // and the event that a human denied would carry a billing refusal instead.
    let fixtures = Fixtures::create_with_queue().await;
    let acquired = received(&fixtures).await;
    let fleet = &acquired.fleet_id;
    let event = &acquired.event_id;
    let leases = fixtures.leases();

    let first = leases
        .block(
            &acquired,
            Refusal {
                label: FIRST_LABEL,
                detail: DETAIL,
            },
        )
        .await
        .expect("the first refusal must write");
    let second = leases
        .block(&acquired, Refusal::labelled(SECOND_LABEL))
        .await
        .expect_err("a settled admission refuses another mutation");

    assert!(matches!(first, Ended::Now(_)));
    // The settled admission refuses a second terminal mutation.
    assert_eq!(second.code(), afd_core::error_code::RUN_STALE_FENCING_TOKEN);
    let (label, detail) = failure_of(&fixtures, fleet.as_str(), event).await;
    assert_eq!(label, FIRST_LABEL, "the first refusal must stand");
    assert_eq!(detail.as_deref(), Some(DETAIL));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_refusal_with_no_instruction_stores_null_rather_than_empty() {
    // The row shape a consumer tests with `IS NULL`. An empty string would
    // pass every `IS NOT NULL` check and render as a blank instruction in the
    // dashboard, which reads as "we have nothing to tell you" rather than as
    // "there was nothing to tell".
    let fixtures = Fixtures::create_with_queue().await;
    let acquired = received(&fixtures).await;
    let fleet = &acquired.fleet_id;
    let event = &acquired.event_id;

    fixtures
        .leases()
        .block(&acquired, Refusal::labelled(FIRST_LABEL))
        .await
        .expect("the refusal must write");

    let (label, detail) = failure_of(&fixtures, fleet.as_str(), event).await;
    assert_eq!(label, FIRST_LABEL);
    assert_eq!(detail, None, "an empty detail must store as SQL NULL");
}
