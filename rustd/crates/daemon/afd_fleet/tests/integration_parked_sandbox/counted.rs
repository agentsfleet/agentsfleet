//! Who won an assignment on a parked fleet, as the parked-assignments counter records it.
//! The keeper's assignment counts only when its event resumes the park, so it is
//! read through the run the assignment hands out.
//!
//! The counter is process-wide and the suite's tests run in parallel, so a
//! assignment here is read as a movement of its own series rather than an exact
//! total: other parked-sandbox tests assign parked fleets at the same time.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_runner::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_PARK_IDLE};

use af_observability::test_util::Capture;
use afd_fleet::run::{Acquired, Billed, Kind, Runs};
use jiff::Timestamp;

use super::{live, park};
use crate::rows::{LAPSE, Row};
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// The family a won assignment on a parked fleet counts under.
const PARKED_ASSIGNMENTS: &str = "agentsfleet_run_parked_assignments_total";

/// Its one label, and the two outcomes it names.
const OUTCOME: &str = "outcome";
const KEEPER: &str = "keeper";
const OTHER_AFTER_LAPSE: &str = "other_after_lapse";

/// Assignments one test makes in a row. Well past every parked assignment a neighbour in
/// this suite makes, so a counter moved by neighbours cannot pass for them.
const ASSIGNMENTS_IN_A_ROW: u64 = 16;

/// When a park made here lapses: after every assignment one test makes in a row.
fn until(fixtures: &Fixtures) -> Timestamp {
    fixtures.after(SANDBOX_PARK_IDLE)
}

/// The series `outcome` names, read now.
fn assignments(capture: &Capture, outcome: &str) -> u64 {
    capture.sum(PARKED_ASSIGNMENTS, &[(OUTCOME, outcome)])
}

/// `runner` wins `fleet`'s thread.
async fn wins(runs: &Runs, fleet: &Uuid7, runner: &Uuid7) {
    runs.assign(fleet, runner, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("the thread is won");
}

/// A fleet holding an event, whose one runner is live and parks its sandbox.
async fn parked(fixtures: &Fixtures) -> Seeded<1> {
    let seeded = seeded::<1>(fixtures).await;
    let [keeper] = &seeded.runners;
    live(fixtures, keeper).await;
    park(
        &fixtures.runs(),
        fixtures,
        &seeded.fleet,
        keeper,
        until(fixtures),
    )
    .await;
    seeded
}

/// The keeper runs its parked fleet at `now`, and the run row is written,
/// so the run left to lapse is one the next assignment reassigns.
async fn runs_at(runs: &Runs, seeded: &Seeded<1>, now: Timestamp) -> Acquired {
    let [keeper] = &seeded.runners;
    let acquired = seed::select_fleet(runs, keeper, &seeded.fleet)
        .await
        .expect("the keeper runs its parked fleet");
    let billed = Billed {
        tenant_id: &seeded.tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    runs.issue(keeper, &acquired, billed, now)
        .await
        .expect("the run row is written")
        .expect("the assignment is still held");
    acquired
}

/// The keeper's fresh event resumes its live park, and counts as the
/// keeper's assignment.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_keepers_assignment_counts_as_the_keepers() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let seeded = parked(&fixtures).await;
    let before = assignments(&capture, KEEPER);

    let fresh = runs_at(&fixtures.runs(), &seeded, fixtures.enrolled_at()).await;

    let after = assignments(&capture, KEEPER);
    assert!(fresh.resume_park, "the fresh event resumes the park");
    assert!(after > before, "{before} -> {after}");
    fixtures.cleanup().await;
}

/// The keeper's reassignments on its live park count nothing: each starts its
/// event clean, since the first attempt may have run in the parked sandbox.
/// Bounded for the module note's reason: the reassignments move the keeper
/// series by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_keepers_reassignments_on_a_live_park_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let seeded = parked(&fixtures).await;
    let runs = fixtures.runs();
    let first = runs_at(&runs, &seeded, fixtures.enrolled_at()).await;
    assert_eq!(first.kind, Kind::Fresh);
    let before = assignments(&capture, KEEPER);

    for _reassignment in 0..ASSIGNMENTS_IN_A_ROW {
        // The keeper's run outlives its thread: the next assignment reassigns it.
        fixtures
            .age(Row::Thread(&seeded.fleet), "run_expires_at", LAPSE)
            .await;
        let run = runs_at(&runs, &seeded, fixtures.enrolled_at()).await;
        assert_eq!(run.kind, Kind::Reassigned);
        assert!(!run.resume_park, "a reassignment builds fresh");
    }

    let moved = assignments(&capture, KEEPER) - before;
    assert!(
        moved < ASSIGNMENTS_IN_A_ROW,
        "{ASSIGNMENTS_IN_A_ROW} reassignments on a live park moved the keeper series by {moved}"
    );
    fixtures.cleanup().await;
}

/// The lapse is the keeper's: it fell silent while its park stood. A park
/// past its own deadline is no park, and an assignment over it counts nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_assignment_after_a_lapse_counts_as_other_after_lapse() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper, other]) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    let before = assignments(&capture, OTHER_AFTER_LAPSE);

    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&keeper), "last_seen_at", silence)
        .await;
    wins(&runs, &fleet, &other).await;

    let after = assignments(&capture, OTHER_AFTER_LAPSE);
    assert!(after > before, "{before} -> {after}");
    fixtures.cleanup().await;
}

/// Assignments on a fleet nobody parks count under neither outcome. Bounded
/// rather than exact, for the reason the module note gives: the assignments made
/// here move the counter by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_assignments_on_an_unparked_fleet_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, runners) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let before = (
        assignments(&capture, KEEPER),
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
        assignments(&capture, KEEPER) - before.0,
        assignments(&capture, OTHER_AFTER_LAPSE) - before.1,
    );
    assert!(
        moved.0 < ASSIGNMENTS_IN_A_ROW && moved.1 < ASSIGNMENTS_IN_A_ROW,
        "{ASSIGNMENTS_IN_A_ROW} unparked assignments moved the parked-assignments counter by {moved:?}"
    );
    fixtures.cleanup().await;
}
