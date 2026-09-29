//! A recovery cursor whose fleet was deleted mid-rotation.
//!
//! Split from `integration_recovery_progress` by concern; that file's header
//! explains why these reproduce with a budget of ONE fleet and count passes
//! against the deployment's own unfinished fleets.

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
    EVERY_ROW, NO_GRACE, RECOVERY_LANE, admission, ledger, producer_key,
};
use crate::integration_recovery_progress::{ONE_FLEET, SLACK_PASSES, unfinished_fleets};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// Removes every trace of one fleet's accepted work from the ledger.
///
/// What a deleted fleet looks like to these scans: the rows are the only thing
/// that put a `fleet_id` in front of them, so dropping the rows drops the
/// fleet. The cursor, filed on the pass that just read it, now names a value
/// no row carries.
async fn forget_fleet_rows(fixtures: &Fixtures, fleet: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query("DELETE FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
        .bind(fleet)
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|_| panic!("deleting {fleet}'s admissions"));
}

/// A cursor whose fleet has been deleted carries on, against the real scan.
///
/// The unit sibling in `afd_admission` proves the resume state hands back the
/// value it was given. It cannot prove what the scan does with that value,
/// because it never binds one: the comparison it asserts on is its own. This
/// binds [`Admissions::reconcile`] to a live ledger and deletes the fleet the
/// cursor is sitting on, which is the case the keyset design exists for —
/// storing a row identifier and looking it up would stall here, with a cursor
/// pointing at nothing and a scan with nowhere to resume from.
///
/// The teeth are in the bound, not the slack. Every fleet here stays unfinished
/// after its repair — the replay sweeper re-appends with a live receipt and
/// nothing delivers it — so a pass that restarted at the lowest-sorting row
/// would spend every remaining pass on the same fleet and never reach the one
/// sorting last, however many passes it ran.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_deleted_fleet_does_not_strand_the_ones_after_it() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    // Three fleets holding accepted work the queue then lost, in id order:
    // the first is the one that will be deleted out from under the cursor.
    let mut lost = Vec::new();
    for which in 0..3 {
        let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
        let key = producer_key(&fleet, &format!("deleted-cursor-{which}"));
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        streams
            .forget(&fleet)
            .await
            .expect("destroying this fleet's stream data");
        lost.push((fleet, event.stored.id));
    }
    lost.sort_by(|left, right| left.0.cmp(&right.0));

    // Walk until the lowest-sorting of the three has been repaired, which is
    // the pass that filed a cursor naming it.
    let passes = unfinished_fleets(&fixtures).await + SLACK_PASSES;
    let mut progress = Progress::default();
    let (deleted, deleted_event) = lost.remove(0);
    for _pass in 0..passes {
        let at = clock::now();
        live.reconcile(at, ONE_FLEET, EVERY_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
        let repaired = match fixtures.admission_receipt(&deleted, &deleted_event).await {
            Some(receipt) => streams
                .holds_entry(&deleted, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            None => false,
        };
        if repaired {
            break;
        }
    }

    // ── The deletion. The cursor now names a fleet no row carries.
    forget_fleet_rows(&fixtures, &deleted).await;

    let remaining = unfinished_fleets(&fixtures).await + SLACK_PASSES;
    for _pass in 0..remaining {
        let at = clock::now();
        live.reconcile(at, ONE_FLEET, EVERY_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs past a deleted fleet");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs past a deleted fleet");
    }

    for (fleet, event_id) in &lost {
        let receipt = fixtures
            .admission_receipt(fleet, event_id)
            .await
            .unwrap_or_else(|| panic!("{event_id} is accepted work and must hold a receipt"));
        assert!(
            streams
                .holds_entry(fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            "{event_id} sorts after the deleted fleet and was still recovered"
        );
    }

    fixtures.cleanup().await;
}
