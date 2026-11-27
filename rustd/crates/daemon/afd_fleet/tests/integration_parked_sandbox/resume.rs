//! Whether a run tells its runner to resume the sandbox it parks: only
//! when the keeper's own assignment found its park live, and only for a fresh
//! event. Anything else builds fresh, because the parked sandbox may predate
//! another runner's run or the event's own first attempt.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_runner::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_PARK_IDLE};

use afd_fleet::run::{Billed, Kind, Runs};
use jiff::Timestamp;

use super::{live, park};
use crate::rows::{LAPSE, Row};
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// When a live park made here lapses.
fn until(fixtures: &Fixtures) -> Timestamp {
    fixtures.after(SANDBOX_PARK_IDLE)
}

/// Whether `runner`'s won assignment on `fleet` resumes its park.
async fn resumes(runs: &Runs, fleet: &Uuid7, runner: &Uuid7) -> bool {
    runs.assign(fleet, runner, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("the thread is won")
        .resume_park
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_keepers_assignment_on_a_live_park_resumes_it() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;

    assert!(resumes(&runs, &fleet, &keeper).await);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_after_the_park_lapsed_builds_fresh() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let (never_parked, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    let lapse = SANDBOX_PARK_IDLE.saturating_mul(2);
    fixtures
        .age(Row::Thread(&fleet), "park_expires_at", lapse)
        .await;

    assert!(!resumes(&runs, &fleet, &keeper).await, "lapsed");
    assert!(
        !resumes(&runs, &never_parked, &keeper).await,
        "a thread whose last report recorded no park"
    );
    fixtures.cleanup().await;
}

/// The keeper fell silent, another runner ran the fleet and reported no
/// park, and the keeper came back: neither assignment resumes anything.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_after_another_runner_ran_the_fleet_builds_fresh() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper, other]) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&keeper), "last_seen_at", silence)
        .await;

    let others = runs
        .assign(&fleet, &other, RUN_TTL)
        .await
        .expect("the assignment answers")
        .expect("a silent keeper's fleet is won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    runs.release_through(&mut connection, &fleet, others.fence, None)
        .await
        .expect("the other runner's report lands");
    drop(connection);

    assert!(!others.resume_park, "the other runner parked nothing");
    assert!(
        !resumes(&runs, &fleet, &keeper).await,
        "the keeper's sandbox predates the other runner's run"
    );
    fixtures.cleanup().await;
}

/// The keeper's fresh run resumes its park; when that run lapses
/// unreported, the event's reassignment builds fresh even though the park the
/// thread records is still live, because the first attempt may have run in it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_reassigned_event_builds_fresh_on_a_live_park() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [keeper],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    let now = fixtures.enrolled_at();

    let fresh = seed::select_fleet(&runs, &keeper, &fleet)
        .await
        .expect("the keeper runs its parked fleet");
    let billed = Billed {
        tenant_id: &tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    runs.issue(&keeper, &fresh, billed, now)
        .await
        .expect("the run row is written")
        .expect("the assignment is still held");
    fixtures
        .age(Row::Thread(&fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = seed::select_fleet(&runs, &keeper, &fleet)
        .await
        .expect("a lapsed run is reassignable");

    assert!(fresh.resume_park, "the fresh event resumes the live park");
    assert_eq!(reassigned.kind, Kind::Reassigned);
    assert!(!reassigned.resume_park, "the reassigned event builds fresh");
    fixtures.cleanup().await;
}
