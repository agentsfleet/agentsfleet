//! Whether a run tells its runner to resume the sandbox it holds: only
//! when the holder's own assignment found its hold live, and only for a fresh
//! event. Anything else builds fresh, because the held sandbox may predate
//! another runner's run or the event's own first attempt.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_core::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};
use afd_fleet::run::{Billed, Kind, Runs};
use jiff::Timestamp;

use super::{hold, live};
use crate::rows::{LAPSE, Row};
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// When a live hold made here lapses.
fn until(fixtures: &Fixtures) -> Timestamp {
    fixtures.after(SANDBOX_HOLD_IDLE)
}

/// Whether `runner`'s won assignment on `fleet` resumes its hold.
async fn resumes(runs: &Runs, fleet: &Uuid7, runner: &Uuid7) -> bool {
    runs.assign(fleet, runner, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("the thread is won")
        .resume_hold
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_assignment_on_a_live_hold_resumes_it() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until(&fixtures)).await;

    assert!(resumes(&runs, &fleet, &holder).await);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_after_the_hold_lapsed_builds_fresh() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let (never_held, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until(&fixtures)).await;
    let lapse = SANDBOX_HOLD_IDLE.saturating_mul(2);
    fixtures
        .age(Row::Thread(&fleet), "hold_expires_at", lapse)
        .await;

    assert!(!resumes(&runs, &fleet, &holder).await, "lapsed");
    assert!(
        !resumes(&runs, &never_held, &holder).await,
        "a thread whose last report recorded no hold"
    );
    fixtures.cleanup().await;
}

/// The holder fell silent, another runner ran the fleet and reported no
/// hold, and the holder came back: neither assignment resumes anything.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_after_another_runner_ran_the_fleet_builds_fresh() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until(&fixtures)).await;
    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&holder), "last_seen_at", silence)
        .await;

    let others = runs
        .assign(&fleet, &other, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("a silent holder's fleet is won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    runs.release_through(&mut connection, &fleet, others.fence, None)
        .await
        .expect("the other runner's report lands");
    drop(connection);

    assert!(!others.resume_hold, "the other runner held nothing");
    assert!(
        !resumes(&runs, &fleet, &holder).await,
        "the holder's sandbox predates the other runner's run"
    );
    fixtures.cleanup().await;
}

/// The holder's fresh run resumes its hold; when that run lapses
/// unreported, the event's reassignment builds fresh even though the hold the
/// thread records is still live, because the first attempt may have run in it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_reassigned_event_builds_fresh_on_a_live_hold() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [holder],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until(&fixtures)).await;
    let now = fixtures.enrolled_at();

    let fresh = seed::select_fleet(&runs, &holder, &fleet)
        .await
        .expect("the holder runs its held fleet");
    let billed = Billed {
        tenant_id: &tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    runs.issue(&holder, &fresh, billed, now)
        .await
        .expect("the run row is written")
        .expect("the assignment is still held");
    fixtures
        .age(Row::Thread(&fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = seed::select_fleet(&runs, &holder, &fleet)
        .await
        .expect("a lapsed run is reassignable");

    assert!(fresh.resume_hold, "the fresh event resumes the live hold");
    assert_eq!(reassigned.kind, Kind::Reassigned);
    assert!(!reassigned.resume_hold, "the reassigned event builds fresh");
    fixtures.cleanup().await;
}
