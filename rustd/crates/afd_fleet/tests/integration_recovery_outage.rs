//! What the two recovery sweepers do when the QUEUE is the thing that is down.
//!
//! The recovery suite next door proves the repairs a healthy pass makes. This
//! proves the arm those passes take when the datastore they repair THROUGH is
//! unreachable — which is not an error either sweeper raises. A replay that
//! cannot append leaves the row owed and stops the pass; a reconcile that
//! cannot probe keeps the receipt it could not disprove. Both are the answer
//! that changes nothing, and both are what a deployment meets during a queue
//! outage rather than a rare interleaving.
//!
//! The dead queue is a handle this test owns (`queue::unreachable`), never a
//! paused container: the lane's Dragonfly is shared by every binary running in
//! parallel, so taking the server away would fail unrelated suites.
//!
//! Marked `#[ignore]` for the reason the sibling suite states: only
//! `make test-integration-rustd` runs these.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::clock;
use afd_wire::event::EventType;

use afd_admission::Progress;

use crate::integration_admission_recovery::{EVERY_FLEET, EVERY_ROW, NO_GRACE, RECOVERY_LANE};
use crate::queue;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

const ACTOR: &str = "webhook:outage";
const REQUEST_JSON: &str = r#"{"delivery":"outage"}"#;

/// A ledger over the lane's database and a queue that is not there.
fn deferring(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), queue::unreachable())
}

/// A ledger over both of the lane's live datastores.
fn live(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), fixtures.queue().clone())
}

/// One webhook admission, keyed off the fleet so it cannot collide with
/// another run's — `uq_fleet_admissions_producer_key` is unique across the
/// whole table, not per fleet.
fn admission<'a>(fleet: &'a str, workspace: &'a str, key: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: REQUEST_JSON,
        reply: afd_admission::Reply::None,
    }
}

/// A replay pass whose queue is down appends nothing and leaves the row owed.
///
/// Not an error: the rows keep their NULL receipt, the count says how far the
/// pass got, and the next pass retries. The pass also STOPS at the first
/// refusal rather than walking the rest of the batch into the same one — a
/// queue that refused one append will refuse the next, and the rows are
/// equally owed either way.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replay_pass_against_a_dead_queue_repairs_nothing_and_raises_nothing() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<1>(&fixtures).await;

    // The row commits whatever the queue does, which is what leaves it owed.
    let key = format!("{fleet}:replay-outage");
    let deferred = deferring(&fixtures)
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("the row commits whatever the queue does");
    assert_eq!(
        fixtures.admission_receipt(&fleet, &deferred.id).await,
        None,
        "an append that never happened records no receipt"
    );

    // The real clock, not a fixture instant: `replay` cuts off against
    // `created_at`, which `admit` stamps with `clock::now()`.
    let replayed = deferring(&fixtures)
        .replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("a queue outage is the pass's outcome, never its error");

    assert_eq!(
        replayed.appended, 0,
        "nothing can be appended to a queue that is not there: {replayed:?}"
    );
    assert_eq!(
        fixtures.admission_receipt(&fleet, &deferred.id).await,
        None,
        "a refused append must not record a receipt for an entry nobody holds"
    );

    // And the row is still repairable: the same pass through a live queue
    // finishes what the outage could not, which is what makes the outage arm
    // a deferral rather than a loss.
    live(&fixtures)
        .replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");
    assert!(
        fixtures
            .admission_receipt(&fleet, &deferred.id)
            .await
            .is_some(),
        "the row the outage deferred must still be repairable afterwards"
    );

    fixtures.cleanup().await;
}

/// A reconcile pass that cannot probe the stream keeps the receipt.
///
/// The probe answers "still there" when it could not be made, because that is
/// the answer that changes nothing: voiding a receipt on a probe that failed
/// would re-append an entry the stream is still holding, and the delivery
/// would happen twice for no reason but an outage.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reconcile_pass_that_cannot_probe_keeps_the_receipt() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<1>(&fixtures).await;

    // A healthy admission: appended AND receipted, so it is exactly the row a
    // reconcile pass examines.
    let key = format!("{fleet}:probe-outage");
    let healthy = live(&fixtures)
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits and receipts in one call");
    let receipt = fixtures
        .admission_receipt(&fleet, &healthy.id)
        .await
        .expect("a live append records its receipt");

    let reconciled = deferring(&fixtures)
        .reconcile(
            clock::now(),
            EVERY_FLEET,
            EVERY_ROW,
            &mut Progress::default(),
        )
        .await
        .expect("an unreachable fleet must not end a pass that has others to examine");
    assert_eq!(
        reconciled.voided, 0,
        "a probe that could not be made must void nothing: {reconciled:?}"
    );
    assert_eq!(
        fixtures.admission_receipt(&fleet, &healthy.id).await,
        Some(receipt),
        "the receipt the probe could not disprove must survive the pass"
    );

    fixtures.cleanup().await;
}
