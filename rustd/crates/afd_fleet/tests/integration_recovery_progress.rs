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

use afd_admission::{Admissions, Progress};
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

/// One row per walk, so every walk that finds anything files a resume point.
const ONE_ROW: i64 = 1;

/// A resume set that holds one fleet, so the second one to need it is refused.
const ROOM_FOR_ONE: usize = 1;

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
            .admission_receipt(&fleet, &event.id)
            .await
            .expect("an append after the loss records its receipt");
        alive.push((event.id, receipt));
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
        .admission_receipt(&fleet, &event.id)
        .await
        .expect("the row was repaired and re-appended");

    // The fleet is whole again. A second pass must find nothing to do on it.
    let settled = live
        .reconcile(clock::now(), EVERY_FLEET, EVERY_ROW, &mut progress)
        .await
        .expect("the second pass runs");

    assert_eq!(
        fixtures.admission_receipt(&fleet, &event.id).await,
        Some(recovered),
        "the second pass left the repaired receipt alone"
    );
    assert_eq!(
        fixtures.admission_replays(&fleet, &event.id).await,
        1,
        "one loss is one re-append, however many passes run"
    );
    assert!(
        !settled.resuming || settled.voided == 0,
        "a settled fleet is not carried as unfinished repair work: {settled:?}"
    );

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

/// Stamps one admission delivered, the way a lease does.
///
/// Bound here rather than driven through `Leases::record_received` because what
/// this test needs from the runner is only the effect — the fleet's oldest
/// undelivered row moving on — and standing a whole lease up to get it would
/// make a test about recovery a test about leasing.
async fn deliver(fixtures: &Fixtures, fleet: &str, event_id: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    let (created_at, seq) = event_id
        .split_once('-')
        .expect("a logical event id is `<created_at>-<seq>`");
    sqlx::query(afd_admission::sql::MARK_DELIVERED)
        .bind(fleet)
        .bind(created_at.parse::<i64>().expect("the instant is numeric"))
        .bind(seq.parse::<i64>().expect("the sequence is numeric"))
        .bind(clock::now().as_millis())
        .execute(&mut *connection)
        .await
        .expect("stamping the delivery");
}

/// A ledger whose database will not answer, over the lane's real queue.
///
/// `Db::unreachable` builds the pool lazily and opens no socket, so the failure
/// arrives on the first statement rather than at construction — which is what
/// makes it a mid-pass failure rather than a setup error.
fn dead_ledger(fixtures: &Fixtures) -> Admissions {
    let environment = afd_core::env::MapEnv::from_pairs([(
        afd_db::config::DbRole::Api.url_knob(),
        "postgres://nowhere/agentsfleet",
    )]);
    let pool = afd_db::config::PoolConfig::resolve(&environment, afd_db::config::DbRole::Api)
        .expect("a lazy pool config resolves");
    Admissions::for_tests(afd_db::Db::unreachable(&pool), fixtures.queue().clone())
}

/// A pass that fails partway keeps the repairs it drained but never walked.
///
/// `resume_repairs` drains rather than borrows, so a pass holding resume points
/// holds the only copy of them. Returning the database error straight through
/// dropped every fleet the pass had not reached — and a dropped resume point is
/// not a slower repair, it is a fleet back under the head probe, which after a
/// partial repair cannot see the rows that are still lost.
///
/// Staged with two ledgers over ONE resume state: the first files a repair
/// against the live lane, the second meets a database that will not answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_pass_keeps_the_repairs_it_drained() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut owed = Vec::new();
    for which in 0..3 {
        let key = producer_key(&fleet, &format!("drained-{which}"));
        owed.push(
            live.admit(admission(&fleet, &workspace, &key))
                .await
                .expect("a live queue admits and receipts")
                .id,
        );
    }
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    // One row per walk, so the first pass fills its batch and files where it
    // stopped rather than reaching the end of the fleet.
    let mut progress = Progress::with_capacity(ROOM_FOR_ONE);
    live.reconcile(clock::now(), EVERY_FLEET, ONE_ROW, &mut progress)
        .await
        .expect("the first pass runs against both live datastores");
    assert!(
        progress.is_resuming(),
        "the first pass stopped mid-fleet and filed where to carry on"
    );

    // The second pass takes that repair out of the set and then cannot walk it.
    let failed = dead_ledger(&fixtures)
        .reconcile(clock::now(), EVERY_FLEET, ONE_ROW, &mut progress)
        .await;
    assert!(
        failed.is_err(),
        "a database that will not answer fails the pass: {failed:?}"
    );
    assert!(
        progress.is_resuming(),
        "the drained repair went back; losing it would put this fleet under the \
         head probe, which cannot see what it has left to recover"
    );

    // And the repair really does carry on, against a ledger that answers again.
    for _pass in 0..4 {
        let at = clock::now();
        live.reconcile(at, EVERY_FLEET, ONE_ROW, &mut progress)
            .await
            .expect("the recovering pass runs");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs");
    }
    for event_id in &owed {
        let receipt = fixtures
            .admission_receipt(&fleet, event_id)
            .await
            .unwrap_or_else(|| panic!("{event_id} is accepted work and must hold a receipt"));
        assert!(
            streams
                .holds_entry(&fleet, &EventId::of(&receipt))
                .await
                .expect("the stream answers"),
            "{event_id} recovered after the pass that failed mid-walk"
        );
    }

    fixtures.cleanup().await;
}
