//! The two batch boundaries recovery used to stop at.
//!
//! Its sibling [`integration_admission_recovery`](crate::integration_admission_recovery)
//! proves that a loss is recovered. It proves it with `EVERY_FLEET` and
//! `EVERY_ROW` — limits larger than anything it admits — so every pass sees the
//! whole loss and the pass's own caps are never reached. That is the right
//! shape for the question it asks and the reason neither failure below was
//! ever caught: both live strictly PAST a cap.
//!
//! # The two failures, and why a cap is not a pace without a cursor
//!
//! **More unfinished fleets than one pass examines.** The scan orders by
//! `fleet_id` — `DISTINCT ON` requires it — so without a cursor every pass
//! reads the same lowest-sorting fleets. Healthy ones still spend a slot, so a
//! deployment busy enough never examines the fleets sorting after them, and
//! accepted work on one of them is never recovered at all.
//!
//! **More lost rows on one fleet than one walk repairs.** The pass decides a
//! fleet is worth walking by asking about its OLDEST undelivered receipt. Void
//! a batch, let the replay sweeper re-append those rows with live receipts, and
//! that question now has the wrong answer: the oldest undelivered row is one
//! the repair just fixed, the stream holds it, and the fleet reports healthy
//! over rows that are still lost.
//!
//! # How these reproduce at a scale a suite can run
//!
//! By shrinking the caps rather than growing the data. The driver ships 128
//! fleets and 32 rows; the branch that fails is `read == cap`, and it fails the
//! same way at 1 and at 2. A test that seeded 129 fleets would prove nothing
//! extra and would spend a minute doing it.
//!
//! The fleet test therefore runs with a budget of ONE and counts its passes
//! against the deployment's own unfinished-fleet count, because these scans
//! carry no fleet predicate: a sibling suite's deferred row is in the rotation
//! too, and the rotation is what is being graded. Read once, up front, under
//! [`RECOVERY_LANE`].
//!
//! Marked `#[ignore]` for the reason the sibling gives: `make test-unit-rustd`
//! compiles and lints these, and `make test-integration-rustd` runs them.
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

/// One fleet per pass, which is the smallest budget that still examines
/// anything.
///
/// The cap under test. At one, a pass that reads a fleet has filled its budget,
/// so the cursor advances on every pass and the rotation is exercised at its
/// most demanding — every fleet in the deployment costs a pass of its own.
const ONE_FLEET: i64 = 1;

/// Two rows per walk, which is the smallest batch that can leave a remainder.
const TWO_ROWS: i64 = 2;

/// Passes to spend beyond the deployment's own unfinished fleets.
///
/// Slack, not a guess at the answer: the rotation needs one pass per unfinished
/// fleet plus one to wrap, and a sibling suite admitting a row mid-test adds to
/// that count after it was read. Small enough that a design which does NOT
/// rotate still fails — the failing one never reaches these fleets however many
/// passes it runs.
const SLACK_PASSES: usize = 8;

/// How many distinct fleets in the whole deployment hold receipted,
/// undelivered work.
///
/// The rotation's period. Read directly rather than inferred from a pass's
/// counters, because `Reconciled::probed` is every suite's business at once —
/// the point the sibling file makes about never asserting a pass count.
async fn unfinished_fleets(fixtures: &Fixtures) -> usize {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let counted: (i64,) = sqlx::query_as(
        "SELECT count(DISTINCT fleet_id) FROM core.fleet_admissions
          WHERE receipt IS NOT NULL AND delivered_at IS NULL",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("counting the deployment's unfinished fleets");
    usize::try_from(counted.0).expect("a fleet count fits a usize")
}

/// Every fleet holding undelivered work is examined, however many there are.
///
/// The first failure, reproduced at its own boundary. Three fleets each hold
/// one accepted admission whose stream is then destroyed, and the pass is given
/// a budget of ONE fleet. Without a cursor, every pass reads whichever fleet
/// sorts lowest in the whole deployment and repairs at most that one — the
/// other two are unreachable for as long as the first is unfinished, which
/// after its repair it remains until the replay sweeper drains it.
///
/// With the cursor, one pass examines one fleet and the next starts after it,
/// so the rotation covers the deployment and all three recover.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn rotation_recovers_fleets_past_the_pass_budget() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    // Three fleets, each holding one admission the queue accepted and then lost.
    let mut lost = Vec::new();
    for which in 0..3 {
        let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
        let key = producer_key(&fleet, &format!("rotation-{which}"));
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        assert!(
            fixtures
                .admission_receipt(&fleet, &event.id)
                .await
                .is_some(),
            "{key} was receipted before the loss"
        );
        streams
            .forget(&fleet)
            .await
            .expect("destroying this fleet's stream data");
        lost.push((fleet, event.id));
    }

    // One pass per unfinished fleet covers the rotation; the slack absorbs a
    // sibling suite adding one after the count was read.
    let passes = unfinished_fleets(&fixtures).await + SLACK_PASSES;
    let mut progress = Progress::default();
    for _pass in 0..passes {
        let at = clock::now();
        live.reconcile(at, ONE_FLEET, EVERY_ROW, &mut progress)
            .await
            .expect("the reconcile pass runs against both live datastores");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs against both live datastores");
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
            "{event_id} sorts past the pass budget and was still recovered"
        );
    }

    fixtures.cleanup().await;
}

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
        owed.push(event.id);
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
