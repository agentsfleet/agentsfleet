//! Recovery beside live traffic: rows the rebuilt stream still holds, two
//! reconcilers over one loss, and a pass over a fleet with nothing wrong.
//!
//! Split from `integration_recovery_progress` by concern.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, naming \
              which of several fleets or rows it was"
)]

use afd_admission::Progress;
use afd_core::clock;
use afd_dragonfly::{EventId, FleetStreams};

use crate::integration_admission_recovery::{
    EVERY_FLEET, EVERY_ROW, NO_GRACE, RECOVERY_LANE, admission, ledger, producer_key,
};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// Rows the stream still holds are walked past, never voided.
///
/// A rebuilt stream is not an empty one. After the loss this fleet takes new
/// admissions, which append to a fresh stream and get LIVE receipts, and the
/// walk meets both kinds in one batch: the old rows whose entries are gone and
/// the new ones sitting right there. Voiding a fleet wholesale would re-append
/// work that is already queued, so the walk asks per row and skips the ones
/// that answer yes — which is the branch this grades.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn interleaved_live_admissions_survive_recovery() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut lost = Vec::new();
    for which in 0..2 {
        let key = producer_key(&fleet, &format!("interleaved-lost-{which}"));
        lost.push(
            live.admit(admission(&fleet, &workspace, &key))
                .await
                .expect("a live queue admits and receipts")
                .stored
                .id,
        );
    }
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    // Admitted AFTER the loss, so the append rebuilds the stream and these
    // receipts name entries that are really there.
    let mut alive = Vec::new();
    for which in 0..2 {
        let key = producer_key(&fleet, &format!("interleaved-live-{which}"));
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        let receipt = fixtures
            .admission_receipt(&fleet, &event.stored.id)
            .await
            .expect("an append after the loss records its receipt");
        alive.push((event.stored.id, receipt));
    }

    let mut progress = Progress::default();
    for _pass in 0..4 {
        let at = clock::now();
        live.reconcile(at, EVERY_FLEET, EVERY_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
    }

    for (event_id, receipt) in &alive {
        assert_eq!(
            fixtures.admission_receipt(&fleet, event_id).await.as_ref(),
            Some(receipt),
            "{event_id} was queued and alive; recovery must not have touched it"
        );
        assert_eq!(
            fixtures.admission_replays(&fleet, event_id).await,
            0,
            "{event_id} was never re-appended, because it was never lost"
        );
    }
    for event_id in &lost {
        let receipt = fixtures
            .admission_receipt(&fleet, event_id)
            .await
            .unwrap_or_else(|| panic!("{event_id} is accepted work and must hold a receipt"));
        assert!(
            streams
                .holds_entry(&fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            "{event_id} was lost beside live work and was still recovered"
        );
    }

    fixtures.cleanup().await;
}

/// Two reconcilers over one lost fleet repair each row once.
///
/// Both read the same candidates and both probe them; the repair is decided by
/// `VOID_LOST_RECEIPT` pinning the receipt it was told about, so the second
/// write matches nothing and reports the zero it did. The sum is what the
/// assertion is on: duplicated round trips are the accepted cost, a duplicated
/// repair is not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_reconcilers_repair_each_row_once() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut owed = Vec::new();
    for which in 0..3 {
        let key = producer_key(&fleet, &format!("concurrent-{which}"));
        owed.push(
            live.admit(admission(&fleet, &workspace, &key))
                .await
                .expect("a live queue admits and receipts")
                .stored
                .id,
        );
    }
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    let at = clock::now();
    let mut here = Progress::default();
    let mut there = Progress::default();
    let (left, right) = tokio::join!(
        live.reconcile(at, EVERY_FLEET, EVERY_ROW, &mut here),
        live.reconcile(at, EVERY_FLEET, EVERY_ROW, &mut there),
    );
    let left = left.expect("the first pass runs against both live datastores");
    let right = right.expect("the second pass runs against both live datastores");

    assert!(
        left.voided + right.voided <= u64::try_from(owed.len()).expect("three fits a u64"),
        "two passes voided more receipts than the fleet had rows: {left:?} {right:?}"
    );
    live.replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");
    for event_id in &owed {
        assert_eq!(
            fixtures.admission_replays(&fleet, event_id).await,
            1,
            "{event_id} was re-appended exactly once, by whichever pass won"
        );
    }

    fixtures.cleanup().await;
}

/// A pass over a fleet nothing is wrong with voids nothing and reports quiet.
///
/// The idempotency claim, and the one that keeps the pacing honest: a pass that
/// repeats itself must not keep voiding receipts the replay sweeper has already
/// made good, or recovery would cycle a fleet forever at the recovering
/// interval.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn replayed_reconcile_pass_is_idempotent() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let key = producer_key(&fleet, "idempotent");
    let event = live
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits and receipts");
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    let mut progress = Progress::default();
    let at = clock::now();
    live.reconcile(at, EVERY_FLEET, EVERY_ROW, &mut progress)
        .await
        .expect("the repairing pass runs");
    live.replay(at, NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs");
    let recovered = fixtures
        .admission_receipt(&fleet, &event.stored.id)
        .await
        .expect("the row was repaired and re-appended");

    // The fleet is whole again. A second pass must find nothing to do on it.
    let settled = live
        .reconcile(clock::now(), EVERY_FLEET, EVERY_ROW, &mut progress)
        .await
        .expect("the second pass runs");

    assert_eq!(
        fixtures.admission_receipt(&fleet, &event.stored.id).await,
        Some(recovered),
        "the second pass left the repaired receipt alone"
    );
    assert_eq!(
        fixtures.admission_replays(&fleet, &event.stored.id).await,
        1,
        "one loss is one re-append, however many passes run"
    );
    assert!(
        !settled.resuming || settled.voided == 0,
        "a settled fleet is not carried as unfinished repair work: {settled:?}"
    );

    fixtures.cleanup().await;
}
