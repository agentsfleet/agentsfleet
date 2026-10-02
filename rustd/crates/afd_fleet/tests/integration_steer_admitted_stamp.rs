//! A leased steer keeps the place in its thread it was listed under.
//!
//! A thread page merges history rows with the ledger's waiting steers on one
//! key, `(created_at, event_id)`, and a waiting steer sits at its admission
//! instant. When the lease stamped the history row with the lease instant, a
//! steer cut off one page and leased before the next jumped above the cursor
//! that page handed out: the resumed history read skipped it as too new and
//! the resumed waiting read skipped it as delivered. These drive the real
//! lease path, `Leases::record_received`, between two page reads.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::{self, UnixMillis};
use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_dragonfly::FleetStreams;
use afd_events::{Cursor, History, Steer};
use afd_fleet::lease::Delivery;

use crate::integration_admission_recovery::{admission, ledger, producer_key};
use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{seeded_parts, select_fleet_within_rotations};
use crate::support::Fixtures;

/// A person's actor, as a steer records it.
const ACTOR: &str = "steer:user_paging";

/// What they typed, as the route stores it.
const BODY: &str = r#"{"message":"page past me"}"#;

/// How many steers wait: one more than the first page keeps.
const WAITING: usize = 3;

/// The first page keeps the newest two, so the oldest is cut.
const PAGE: i64 = 2;

/// How far past the wall clock the lease lands: far enough that a row stamped
/// with it sorts above every admission this test made.
const LEASE_LATER_MS: i64 = 60_000;

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the fixture mints canonical identifiers")
}

/// A steer cut from page one and leased before page two is still on page two,
/// once, as the history row the lease wrote, at its admission instant.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_leased_between_pages_stays_on_the_resumed_page() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let steer = Steer::new(ledger(&fixtures));
    let mut admitted = Vec::with_capacity(WAITING);
    for _ in 0..WAITING {
        let steered = steer
            .append(&fleet, &workspace, ACTOR, BODY, None)
            .await
            .expect("the steer is admitted");
        admitted.push(steered);
    }
    let oldest = admitted.first().expect("three steers were admitted");
    let history = History::new(fixtures.database.clone());
    let (workspace_id, fleet_id) = (uuid(&workspace), uuid(&fleet));

    let first = history
        .thread_page(&workspace_id, &fleet_id, None, PAGE)
        .await
        .expect("the thread reads");
    let kept: Vec<_> = first
        .into_iter()
        .take(usize::try_from(PAGE).expect("a page size is positive"))
        .collect();
    assert!(
        kept.iter().all(|row| row.row.event_id != oldest.event_id),
        "the oldest steer is cut from page one"
    );
    let last = kept.last().expect("page one is full");
    let cursor = Cursor::after(last.row.created_at, &last.row.event_id);

    let leases = fixtures.leases();
    let polled_at = UnixMillis::from_millis(ENROLLED_AT);
    let acquired = select_fleet_within_rotations(&leases, &runner, polled_at, &fleet)
        .await
        .expect("the fleet holding the steers is leasable");
    assert_eq!(
        acquired.event_id, oldest.event_id,
        "the queue hands out its oldest first"
    );
    let leased_at = clock::now().saturating_add_millis(LEASE_LATER_MS);
    let received = leases
        .record_received(&acquired, leased_at)
        .await
        .expect("the narrative log opens");
    assert_eq!(received.delivery, Delivery::First);
    assert_eq!(
        Some(received.opened_at.as_millis()),
        oldest.admitted_at,
        "the row is stamped with the admission instant, not the lease"
    );

    let second = history
        .thread_page(&workspace_id, &fleet_id, Some(&cursor), PAGE)
        .await
        .expect("the resumed thread reads");
    let found: Vec<_> = second
        .iter()
        .filter(|row| row.row.event_id == oldest.event_id)
        .collect();
    assert_eq!(found.len(), 1, "the leased steer is on page two, once");
    assert_eq!(
        found.first().map(|row| row.row.status.as_str()),
        Some(status::RECEIVED),
        "as the history row the lease wrote"
    );

    FleetStreams::new(fixtures.queue().clone())
        .forget(&fleet)
        .await
        .expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Only a steer moves: a webhook's row keeps the instant the lease passed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_webhook_row_keeps_the_lease_instant() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let key = producer_key(&fleet, "lease-instant");
    ledger(&fixtures)
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits the webhook");

    let leases = fixtures.leases();
    let polled_at = UnixMillis::from_millis(ENROLLED_AT);
    let acquired = select_fleet_within_rotations(&leases, &runner, polled_at, &fleet)
        .await
        .expect("the fleet holding the webhook is leasable");
    let leased_at = clock::now().saturating_add_millis(LEASE_LATER_MS);
    let received = leases
        .record_received(&acquired, leased_at)
        .await
        .expect("the narrative log opens");
    assert_eq!(received.delivery, Delivery::First);
    assert_eq!(received.opened_at, leased_at);

    FleetStreams::new(fixtures.queue().clone())
        .forget(&fleet)
        .await
        .expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
