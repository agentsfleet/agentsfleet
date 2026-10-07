//! Whether a lease tells its runner to resume the sandbox it holds: only
//! when the holder's own claim found its hold live, and only for a fresh
//! event. Anything else builds fresh, because the held sandbox may predate
//! another runner's run or the event's own first attempt.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL_MS, RUNNER_OFFLINE_AFTER_MS, SANDBOX_HOLD_IDLE_MS};
use afd_fleet::lease::{Billed, Kind, Leases};

use super::{at, hold, id, live};
use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// When a live hold made here lapses.
const UNTIL: i64 = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;

/// When a short hold made here lapses.
const SHORT: i64 = ENROLLED_AT + 10;

/// Whether `runner`'s won claim on `fleet` at `now` resumes its hold.
async fn resumes(leases: &Leases, fleet: &str, runner: &Uuid7, now: i64) -> bool {
    leases
        .claim(&id(fleet), runner, at(now), LEASE_TTL_MS)
        .await
        .expect("the claim answers")
        .expect("the slot is won")
        .resume_hold
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_claim_on_a_live_hold_resumes_it() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;

    assert!(resumes(&leases, &fleet, &holder, ENROLLED_AT + 1).await);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_after_the_hold_lapsed_builds_fresh() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let (never_held, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, SHORT).await;

    assert!(
        !resumes(&leases, &fleet, &holder, SHORT + 1).await,
        "lapsed"
    );
    assert!(
        !resumes(&leases, &never_held, &holder, SHORT + 1).await,
        "a slot whose last report recorded no hold"
    );
    fixtures.cleanup().await;
}

/// The holder fell silent, another runner ran the fleet and reported no
/// hold, and the holder came back: neither claim resumes anything.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_after_another_runner_ran_the_fleet_builds_fresh() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;
    let silent = ENROLLED_AT + RUNNER_OFFLINE_AFTER_MS + 1;

    let others = leases
        .claim(&id(&fleet), &other, at(silent), LEASE_TTL_MS)
        .await
        .expect("the claim answers")
        .expect("a silent holder's fleet is won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(&mut connection, &id(&fleet), others.fence, None, at(silent))
        .await
        .expect("the other runner's report lands");
    drop(connection);

    assert!(!others.resume_hold, "the other runner held nothing");
    assert!(
        !resumes(&leases, &fleet, &holder, silent + 1).await,
        "the holder's sandbox predates the other runner's run"
    );
    fixtures.cleanup().await;
}

/// The holder's fresh lease resumes its hold; when that lease lapses
/// unreported, the event's reclaim builds fresh even though the hold the
/// slot records is still live, because the first attempt may have run in it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_reclaimed_event_builds_fresh_on_a_live_hold() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;
    let now = at(ENROLLED_AT + 1);
    let tenant_id = Uuid7::parse(&tenant).expect("a seeded tenant");

    let fresh = seed::select_fleet_within_rotations(&leases, &holder, now, &fleet)
        .await
        .expect("the holder leases its held fleet");
    leases
        .record_received(&fresh, now)
        .await
        .expect("the event's row opens");
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    leases
        .issue(&holder, &fresh, billed, now)
        .await
        .expect("the lease row is written")
        .expect("the claim is still held");
    let lapsed = fresh.leased_until.saturating_add_millis(1);
    let reclaimed = seed::select_fleet_within_rotations(&leases, &holder, lapsed, &fleet)
        .await
        .expect("a lapsed lease is reclaimable");

    assert!(fresh.resume_hold, "the fresh event resumes the live hold");
    assert_eq!(reclaimed.kind, Kind::Reclaim);
    assert!(!reclaimed.resume_hold, "the reclaimed event builds fresh");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
