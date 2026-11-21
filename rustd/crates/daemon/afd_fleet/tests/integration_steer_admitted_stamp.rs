//! A held steer keeps the place in its thread it was listed under.
//!
//! A thread page merges history rows with the ledger's waiting steers on one
//! key, `(created_at, turn_id)`, and a waiting steer sits at its intake
//! instant. When the run stamped the history row with the run instant, a
//! steer cut off one page and held before the next jumped above the cursor
//! that page handed out: the resumed history read skipped it as too new and
//! the resumed waiting read skipped it as delivered. These drive the real
//! run path, the assignment statement, between two page reads.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_core::turn::status;
use afd_fleet::run::Delivery;
use afd_turns::view::{Cursor, History, Steer};

use crate::recovery_seed::{incoming_turn, ledger, producer_key};
use crate::rows::Row;
use crate::seed::{seeded_parts, select_fleet};
use crate::support::Fixtures;

/// A person's actor, as a steer records it.
const ACTOR: &str = "steer:user_paging";

/// What they typed, as the route stores it.
const BODY: &str = r#"{"message":"page past me"}"#;

/// How many steers wait: one more than the first page keeps.
const WAITING: usize = 3;

/// The first page keeps the newest two, so the oldest is cut.
const PAGE: i64 = 2;

/// How far before the run the oldest steer's turn is aged: far enough
/// that a row stamped with the run instant sorts above every turn here.
const ADMITTED_EARLIER: std::time::Duration = std::time::Duration::from_secs(60);

/// A steer cut from page one and held before page two is still on page two,
/// once, as the history row the run wrote, at its intake instant.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_assigned_between_pages_stays_on_the_resumed_page() {
    let fixtures = Fixtures::create().await;
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
    let oldest_id =
        Uuid7::parse(oldest.turn_id.as_str()).expect("the steer names a canonical identifier");
    let turn = Row::Turn(&oldest_id);
    fixtures.age(turn, "created_at", ADMITTED_EARLIER).await;
    let admitted_at = fixtures.instant(turn, "created_at").await;
    let history = History::new(fixtures.database.clone());

    let first = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("the thread reads");
    let kept: Vec<_> = first
        .into_iter()
        .take(usize::try_from(PAGE).expect("a page size is positive"))
        .collect();
    assert!(
        kept.iter().all(|row| row.row.turn_id != oldest_id),
        "the oldest steer is cut from page one"
    );
    let last = kept.last().expect("page one is full");
    let cursor = Cursor::after(last.row.created_at, &last.row.turn_id);

    let runs = fixtures.runs();
    let acquired = select_fleet(&runs, &runner, &fleet)
        .await
        .expect("the fleet holding the steers is leasable");
    assert_eq!(
        acquired.turn_id, oldest_id,
        "the queue hands out its oldest first"
    );
    let received = acquired.opened.received;
    assert_eq!(received.delivery, Delivery::First);
    assert_eq!(
        Some(received.opened_at),
        admitted_at,
        "the row is stamped with the intake instant, not the run"
    );

    let second = history
        .thread_page(&workspace, &fleet, Some(&cursor), PAGE)
        .await
        .expect("the resumed thread reads");
    let found: Vec<_> = second
        .iter()
        .filter(|row| row.row.turn_id == oldest_id)
        .collect();
    assert_eq!(found.len(), 1, "the held steer is on page two, once");
    assert_eq!(
        found.first().map(|row| row.row.status.as_str()),
        Some(status::RECEIVED),
        "as the history row the run wrote"
    );

    fixtures.cleanup().await;
}

/// A webhook's row keeps its intake instant too: every turn sits in its
/// thread where it was received, so no producer's row moves at the assignment.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_webhook_row_keeps_its_intake_instant() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let key = producer_key(&fleet, "run-instant");
    ledger(&fixtures)
        .admit(incoming_turn(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits the webhook");

    let runs = fixtures.runs();
    let (acquired, run_window) = fixtures
        .clock_around(select_fleet(&runs, &runner, &fleet))
        .await;
    let acquired = acquired.expect("the fleet holding the webhook is leasable");
    let admitted_at = fixtures
        .instant(Row::Turn(&acquired.turn_id), "created_at")
        .await;
    let received = acquired.opened.received;
    assert_eq!(received.delivery, Delivery::First);
    assert_eq!(
        Some(received.opened_at),
        admitted_at,
        "the webhook's row carries its intake instant"
    );
    assert!(
        !run_window.contains(&received.opened_at),
        "the assignment does not restamp the row: {} inside {run_window:?}",
        received.opened_at
    );

    fixtures.cleanup().await;
}
