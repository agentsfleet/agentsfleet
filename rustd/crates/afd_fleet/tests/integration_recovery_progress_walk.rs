//! More lost rows on one fleet than one walk repairs, and the resume points
//! that carry a walk across passes.
//!
//! Split from `integration_recovery_progress` by concern; that file's header
//! explains the failure and why a batch of two reproduces it.

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
use crate::integration_recovery_progress::{ONE_ROW, deliver};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// Two rows per walk, which is the smallest batch that can leave a remainder.
const TWO_ROWS: i64 = 2;

/// A resume set that holds one fleet, so the second one to need it is refused.
const ROOM_FOR_ONE: usize = 1;

/// Lost rows past one walk's batch are recovered, not hidden by the repair.
///
/// The second failure, at its own boundary. Three admissions on ONE fleet, the
/// stream destroyed, and a walk that repairs two rows at a time.
///
/// The first pass voids rows one and two; replay re-appends them and records
/// live receipts. Their `created_at` and `seq` belong to the admission and not
/// to the entry, so they keep their place at the head of the undelivered order
/// — and the head is exactly what the next pass's shortcut asks the stream
/// about. Without a resume point that pass is told the fleet is healthy, and
/// row three keeps a receipt naming an entry that no longer exists.
///
/// The extra passes are what make the assertion mean something: the failing
/// design does not recover row three more slowly, it never recovers it while
/// rows one and two remain undelivered.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_walk_resumes_past_the_rows_it_already_repaired() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut owed = Vec::new();
    for which in 0..3 {
        let key = producer_key(&fleet, &format!("batch-{which}"));
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        owed.push(event.stored.id);
    }
    let before_loss: Vec<Option<String>> = {
        let mut receipts = Vec::new();
        for event_id in &owed {
            receipts.push(fixtures.admission_receipt(&fleet, event_id).await);
        }
        receipts
    };
    assert!(
        before_loss.iter().all(Option::is_some),
        "all three were receipted before the loss: {before_loss:?}"
    );

    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    // Pass one repairs rows one and two and files where it stopped. Pass two
    // must carry on from there rather than re-reading a head recovery has
    // already made healthy. The third and fourth are slack.
    let mut progress = Progress::default();
    for _pass in 0..4 {
        let at = clock::now();
        live.reconcile(at, EVERY_FLEET, TWO_ROWS, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
    }

    for (which, (event_id, destroyed)) in owed.iter().zip(&before_loss).enumerate() {
        let receipt = fixtures
            .admission_receipt(&fleet, event_id)
            .await
            .unwrap_or_else(|| panic!("row {which} is accepted work and must hold a receipt"));
        assert_ne!(
            Some(&receipt),
            destroyed.as_ref(),
            "row {which} must hold a NEW receipt: the one it had named a destroyed entry"
        );
        assert!(
            streams
                .holds_entry(&fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            "row {which} sits past the walk's batch and was still recovered"
        );
        assert_eq!(
            fixtures.admission_replays(&fleet, event_id).await,
            1,
            "row {which} was re-appended exactly once, however many passes ran"
        );
    }

    fixtures.cleanup().await;
}

/// A full resume set declines the next repair, says so, and recovers it later.
///
/// The memory bound's cost, paid where it is visible, and the cost is bigger
/// than "slower". Two fleets lose more rows than one walk repairs while the set
/// holds one fleet, so a pass turns one of them away. The turned-away fleet's
/// FIRST lost row is repaired and re-appended; its second is then invisible,
/// because the head probe asks about the oldest undelivered receipt and that is
/// now the live one the repair just made. The fleet is not stranded forever —
/// it is stranded until its restored rows are delivered and the head moves,
/// which on a fleet a runner is consuming is the next lease and on a fleet
/// nobody is consuming is indefinite.
///
/// So this grades three things: the refusal is REPORTED rather than dropped,
/// the row behind it is still owed rather than lost, and delivering the
/// restored rows is what lets the next pass find it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_full_resume_set_declines_a_repair_and_still_recovers_it() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut lost = Vec::new();
    for which in 0..2 {
        let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
        let mut owed = Vec::new();
        for nth in 0..2 {
            let key = producer_key(&fleet, &format!("declined-{which}-{nth}"));
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
        lost.push((fleet, owed));
    }

    // Every fleet examined per pass, so this test does not wait its turn behind
    // a sibling suite's unfinished fleets — the rotation is the other test's
    // subject. One row per walk so both fleets file a resume point, and room
    // for one so the second is refused.
    let mut progress = Progress::with_capacity(ROOM_FOR_ONE);
    let mut declined = 0;
    for _pass in 0..4 {
        let at = clock::now();
        declined += live
            .reconcile(at, EVERY_FLEET, ONE_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores")
            .declined;
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
    }

    assert!(
        declined > 0,
        "a resume set of one, with two fleets losing rows, must have turned one away"
    );

    // Nothing was lost: every row still holds a receipt, live or dead, and a
    // dead one is a row the replay sweeper still owes.
    for (fleet, owed) in &lost {
        for event_id in owed {
            assert!(
                fixtures.admission_receipt(fleet, event_id).await.is_some(),
                "{event_id} is accepted work and must still hold a receipt"
            );
        }
    }

    // The runner does its half: the rows recovery restored are delivered, so
    // each fleet's oldest undelivered receipt is a dead one again and the head
    // probe can see what is still missing.
    for (fleet, owed) in &lost {
        for event_id in owed {
            let receipt = fixtures
                .admission_receipt(fleet, event_id)
                .await
                .expect("every row holds a receipt");
            if streams
                .holds_entry(fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers")
            {
                deliver(&fixtures, fleet, event_id).await;
            }
        }
    }

    for _pass in 0..4 {
        let at = clock::now();
        live.reconcile(at, EVERY_FLEET, ONE_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
    }

    for (fleet, owed) in &lost {
        for event_id in owed {
            let receipt = fixtures
                .admission_receipt(fleet, event_id)
                .await
                .unwrap_or_else(|| panic!("{event_id} is accepted work and must hold a receipt"));
            let held = streams
                .holds_entry(fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers");
            assert!(
                held || fixtures
                    .admission_delivered_at(fleet, event_id)
                    .await
                    .is_some(),
                "{event_id} was on a declined repair and is neither delivered nor re-queued"
            );
        }
    }

    fixtures.cleanup().await;
}
