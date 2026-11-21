//! Who won an assignment on a held fleet, as the held-assignments counter records it.
//! The holder's assignment counts only when its event resumes the hold, so it is
//! read through the run the assignment hands out.
//!
//! The counter is process-wide and the suite's tests run in parallel, so a
//! assignment here is read as a movement of its own series rather than an exact
//! total: other held-sandbox tests assign held fleets at the same time.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_core::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};
use af_observability::test_util::Capture;
use afd_fleet::run::{Acquired, Billed, Kind, Runs};
use jiff::Timestamp;

use super::{hold, live};
use crate::rows::{LAPSE, Row};
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// The family a won assignment on a held fleet counts under.
const HELD_ASSIGNMENTS: &str = "agentsfleet_run_held_assignments_total";

/// Its one label, and the two outcomes it names.
const OUTCOME: &str = "outcome";
const HOLDER: &str = "holder";
const OTHER_AFTER_LAPSE: &str = "other_after_lapse";

/// Assignments one test makes in a row. Well past every held assignment a neighbour in
/// this suite makes, so a counter moved by neighbours cannot pass for them.
const ASSIGNMENTS_IN_A_ROW: u64 = 16;

/// When a hold made here lapses: after every assignment one test makes in a row.
fn until(fixtures: &Fixtures) -> Timestamp {
    fixtures.after(SANDBOX_HOLD_IDLE)
}

/// The series `outcome` names, read now.
fn assignments(capture: &Capture, outcome: &str) -> u64 {
    capture.sum(HELD_ASSIGNMENTS, &[(OUTCOME, outcome)])
}

/// `runner` wins `fleet`'s thread.
async fn wins(runs: &Runs, fleet: &Uuid7, runner: &Uuid7) {
    runs.assign(fleet, runner, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("the thread is won");
}

/// A fleet holding an event, whose one runner is live and holds its sandbox.
async fn held(fixtures: &Fixtures) -> Seeded<1> {
    let seeded = seeded::<1>(fixtures).await;
    let [holder] = &seeded.runners;
    live(fixtures, holder).await;
    hold(
        &fixtures.runs(),
        fixtures,
        &seeded.fleet,
        holder,
        until(fixtures),
    )
    .await;
    seeded
}

/// The holder runs its held fleet at `now`, and the run row is written,
/// so the run left to lapse is one the next assignment reassigns.
async fn runs_at(runs: &Runs, seeded: &Seeded<1>, now: Timestamp) -> Acquired {
    let [holder] = &seeded.runners;
    let acquired = seed::select_fleet(runs, holder, &seeded.fleet)
        .await
        .expect("the holder runs its held fleet");
    let billed = Billed {
        tenant_id: &seeded.tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    runs.issue(holder, &acquired, billed, now)
        .await
        .expect("the run row is written")
        .expect("the assignment is still held");
    acquired
}

/// The holder's fresh event resumes its live hold, and counts as the
/// holder's assignment.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_assignment_counts_as_the_holders() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let seeded = held(&fixtures).await;
    let before = assignments(&capture, HOLDER);

    let fresh = runs_at(&fixtures.runs(), &seeded, fixtures.enrolled_at()).await;

    let after = assignments(&capture, HOLDER);
    assert!(fresh.resume_hold, "the fresh event resumes the hold");
    assert!(after > before, "{before} -> {after}");
    fixtures.cleanup().await;
}

/// The holder's reassignments on its live hold count nothing: each starts its
/// event clean, since the first attempt may have run in the held sandbox.
/// Bounded for the module note's reason: the reassignments move the holder
/// series by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_reassignments_on_a_live_hold_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let seeded = held(&fixtures).await;
    let runs = fixtures.runs();
    let first = runs_at(&runs, &seeded, fixtures.enrolled_at()).await;
    assert_eq!(first.kind, Kind::Fresh);
    let before = assignments(&capture, HOLDER);

    for _reassignment in 0..ASSIGNMENTS_IN_A_ROW {
        // The holder's run outlives its thread: the next assignment reassigns it.
        fixtures
            .age(Row::Thread(&seeded.fleet), "run_expires_at", LAPSE)
            .await;
        let run = runs_at(&runs, &seeded, fixtures.enrolled_at()).await;
        assert_eq!(run.kind, Kind::Reassigned);
        assert!(!run.resume_hold, "a reassignment builds fresh");
    }

    let moved = assignments(&capture, HOLDER) - before;
    assert!(
        moved < ASSIGNMENTS_IN_A_ROW,
        "{ASSIGNMENTS_IN_A_ROW} reassignments on a live hold moved the holder series by {moved}"
    );
    fixtures.cleanup().await;
}

/// The lapse is the holder's: it fell silent while its hold stood. A hold
/// past its own deadline is no hold, and an assignment over it counts nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_assignment_after_a_lapse_counts_as_other_after_lapse() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until(&fixtures)).await;
    let before = assignments(&capture, OTHER_AFTER_LAPSE);

    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&holder), "last_seen_at", silence)
        .await;
    wins(&runs, &fleet, &other).await;

    let after = assignments(&capture, OTHER_AFTER_LAPSE);
    assert!(after > before, "{before} -> {after}");
    fixtures.cleanup().await;
}

/// Assignments on a fleet nobody holds count under neither outcome. Bounded
/// rather than exact, for the reason the module note gives: the assignments made
/// here move the counter by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_assignments_on_an_unheld_fleet_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, runners) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let before = (
        assignments(&capture, HOLDER),
        assignments(&capture, OTHER_AFTER_LAPSE),
    );

    let rounds = usize::try_from(ASSIGNMENTS_IN_A_ROW).expect("sixteen fits a usize");
    for runner in runners.iter().cycle().take(rounds) {
        let assigned = runs
            .assign(&fleet, runner, RUN_TTL)
            .await
            .expect("the assignment answers")
            .expect("a free thread is won");
        runs.release(&fleet, assigned.fence)
            .await
            .expect("an empty assignment lets go");
    }

    let moved = (
        assignments(&capture, HOLDER) - before.0,
        assignments(&capture, OTHER_AFTER_LAPSE) - before.1,
    );
    assert!(
        moved.0 < ASSIGNMENTS_IN_A_ROW && moved.1 < ASSIGNMENTS_IN_A_ROW,
        "{ASSIGNMENTS_IN_A_ROW} unheld assignments moved the held-assignments counter by {moved:?}"
    );
    fixtures.cleanup().await;
}
