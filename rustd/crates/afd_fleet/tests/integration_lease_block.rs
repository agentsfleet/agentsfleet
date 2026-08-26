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

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::admit::Refusal;
use afd_fleet::lease::{Delivery, Ended};
use sqlx::Row as _;

use self::requests::ENROLLED_AT;
use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// The label a first refusal writes.
const FIRST_LABEL: &str = "balance_exhausted";

/// A different label, so a reopened row is distinguishable from a held one.
const SECOND_LABEL: &str = "approval_denied";

/// An operator-readable instruction, to prove it lands beside the label.
const DETAIL: &str = "top up the workspace balance";

/// Two columns of one `core.fleet_events` row, as text.
async fn failure_of(fixtures: &Fixtures, fleet: &str, event: &str) -> (String, Option<String>) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT status, failure_label, failure_detail
           FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(fleet)
    .bind(event)
    .fetch_one(&mut *connection)
    .await
    .expect("the event row must be readable");

    let status: String = row.try_get(0).expect("status is text");
    let label: String = row.try_get(1).expect("failure_label is text");
    let detail: Option<String> = row.try_get(2).expect("failure_detail is nullable text");
    assert_eq!(status, "gate_blocked", "the row must be terminal");
    (label, detail)
}

/// A leased event with its narrative log open, ready to be refused.
async fn received(fixtures: &Fixtures) -> (Uuid7, String) {
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let leases = fixtures.leases();
    let held = leases
        .select(&runner, now)
        .await
        .expect("the selection pass must not fault")
        .expect("the fleet is leasable");
    assert_eq!(
        leases
            .record_received(&held, now)
            .await
            .expect("the narrative log must open"),
        Delivery::First
    );
    let fleet_id = Uuid7::parse(&fleet).expect("the fixture id is a v7 spelling");
    (fleet_id, held.event_id)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_refusal_ends_the_event_and_names_what_refused_it() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, event) = received(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let ended = fixtures
        .leases()
        .block(
            &fleet,
            &event,
            Refusal {
                label: FIRST_LABEL,
                detail: DETAIL,
            },
            now,
        )
        .await
        .expect("the refusal must write");

    assert_eq!(ended, Ended::Now);
    let (label, detail) = failure_of(&fixtures, fleet.as_str(), &event).await;
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
    let (fleet, event) = received(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let leases = fixtures.leases();

    let first = leases
        .block(
            &fleet,
            &event,
            Refusal {
                label: FIRST_LABEL,
                detail: DETAIL,
            },
            now,
        )
        .await
        .expect("the first refusal must write");
    let second = leases
        .block(&fleet, &event, Refusal::labelled(SECOND_LABEL), now)
        .await
        .expect("a second refusal is not a fault");

    assert_eq!(first, Ended::Now);
    // Not an error, and not a write. The acknowledgement is still owed, so the
    // caller proceeds — it just did not decide anything this time.
    assert_eq!(second, Ended::Already);
    let (label, detail) = failure_of(&fixtures, fleet.as_str(), &event).await;
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
    let (fleet, event) = received(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    fixtures
        .leases()
        .block(&fleet, &event, Refusal::labelled(FIRST_LABEL), now)
        .await
        .expect("the refusal must write");

    let (label, detail) = failure_of(&fixtures, fleet.as_str(), &event).await;
    assert_eq!(label, FIRST_LABEL);
    assert_eq!(detail, None, "an empty detail must store as SQL NULL");
}
