//! Recovery progress across a failed pass and a restart.
//!
//! Split from `integration_recovery_progress` by concern: progress lives in
//! memory, and these grade what a failure or a restart costs it.

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
use crate::integration_recovery_progress::{ONE_ROW, deliver};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

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
                .stored
                .id,
        );
    }
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    // A full-size resume set, NOT the one-slot set the declined-repair test
    // uses. These scans carry no fleet predicate, so a sibling suite's lost
    // fleet is walked by this pass too — and with room for one, whichever fleet
    // sorted first would take the slot and this fixture's repair would be the
    // one turned away. That failed in the full workspace run and passed
    // serially, which is the signature of exactly this. One row per walk still,
    // so the first pass fills its batch and files where it stopped.
    let mut progress = Progress::default();
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

/// A restart loses the resume state, and the fleet still recovers afterwards.
///
/// The stated limit of keeping progress in memory, graded rather than asserted
/// in a comment. A process that restarts mid-repair drops every resume point,
/// so its fleets go back to being judged by the head probe — which, after a
/// partial repair, reports a fleet healthy over rows that are still lost. What
/// makes that a delay and not a loss is delivery: once the rows recovery
/// restored are delivered, the head moves to a dead receipt and the probe sees
/// it again.
///
/// The restart is played by dropping the resume state and building a fresh one,
/// which is exactly what the sweeper holds across a process boundary.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_restart_resets_progress_without_losing_coverage() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let streams = FleetStreams::new(fixtures.queue().clone());

    let mut owed = Vec::new();
    for which in 0..3 {
        let key = producer_key(&fleet, &format!("restart-{which}"));
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

    // One pass repairs the first row and files where it stopped.
    let at = clock::now();
    let mut before_restart = Progress::default();
    live.reconcile(at, EVERY_FLEET, ONE_ROW, &mut before_restart)
        .await
        .expect("the first pass runs");
    live.replay(at, NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs");
    assert!(
        before_restart.is_resuming(),
        "the pass stopped mid-fleet and filed where to carry on"
    );

    // ── The restart. Everything that pass remembered is gone.
    drop(before_restart);
    let mut after_restart = Progress::default();

    // The runner does its half: the restored rows are delivered, so this
    // fleet's oldest undelivered receipt is a dead one again.
    for event_id in &owed {
        let receipt = fixtures
            .admission_receipt(&fleet, event_id)
            .await
            .expect("every row holds a receipt");
        if streams
            .holds_entry(&fleet, &EventId::of(&receipt))
            .await
            .expect("the stream answers")
        {
            deliver(&fixtures, &fleet, event_id).await;
        }
    }

    for _pass in 0..6 {
        let at = clock::now();
        live.reconcile(at, EVERY_FLEET, ONE_ROW, &mut after_restart)
            .await
            .expect("a pass after the restart runs");
        live.replay(at, NO_GRACE, EVERY_ROW)
            .await
            .expect("the replay pass runs");
    }

    for event_id in &owed {
        let receipt = fixtures
            .admission_receipt(&fleet, event_id)
            .await
            .unwrap_or_else(|| panic!("{event_id} is accepted work and must hold a receipt"));
        let held = streams
            .holds_entry(&fleet, &EventId::of(&receipt))
            .await
            .expect("the stream answers");
        assert!(
            held || fixtures
                .admission_delivered_at(&fleet, event_id)
                .await
                .is_some(),
            "{event_id} survived a restart mid-repair: it is queued again or delivered"
        );
    }

    fixtures.cleanup().await;
}
