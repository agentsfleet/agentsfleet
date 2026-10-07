//! Who won a claim on a held fleet, as the held-claims counter records it.
//! The holder's claim counts only when its event resumes the hold, so it is
//! read through the lease the claim hands out.
//!
//! The counter is process-wide and the suite's tests run in parallel, so a
//! claim here is read as a movement of its own series rather than an exact
//! total: other held-sandbox tests claim held fleets at the same time.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL_MS, RUNNER_OFFLINE_AFTER_MS, SANDBOX_HOLD_IDLE_MS};
use afd_fleet::lease::{Acquired, Billed, Kind, Leases};
use afd_observability::test_util::Capture;

use super::{at, hold, id, live};
use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// The family a won claim on a held fleet counts under.
const HELD_CLAIMS: &str = "agentsfleet_lease_held_claims_total";

/// Its one label, and the two outcomes it names.
const OUTCOME: &str = "outcome";
const HOLDER: &str = "holder";
const OTHER_AFTER_LAPSE: &str = "other_after_lapse";

/// Claims one test makes in a row. Well past every held claim a neighbour in
/// this suite makes, so a counter moved by neighbours cannot pass for them.
const CLAIMS_IN_A_ROW: u64 = 16;

/// When a hold made here lapses: after every claim one test makes in a row.
const UNTIL: i64 = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;

/// The series `outcome` names, read now.
fn claims(capture: &Capture, outcome: &str) -> u64 {
    capture.sum(HELD_CLAIMS, &[(OUTCOME, outcome)])
}

/// `runner` wins `fleet`'s slot at `now`.
async fn wins(leases: &Leases, fleet: &str, runner: &Uuid7, now: i64) {
    leases
        .claim(&id(fleet), runner, at(now), LEASE_TTL_MS)
        .await
        .expect("the claim answers")
        .expect("the slot is won");
}

/// A fleet holding an event, whose one runner is live and holds its sandbox.
async fn held(fixtures: &Fixtures) -> Seeded<1> {
    let seeded = seeded::<1>(fixtures).await;
    let [holder] = &seeded.runners;
    live(fixtures, holder).await;
    hold(&fixtures.leases(), fixtures, &seeded.fleet, holder, UNTIL).await;
    seeded
}

/// The holder leases its held fleet at `now`, and the lease row is written,
/// so the lease left to lapse is one the next claim reclaims.
async fn leases_at(leases: &Leases, seeded: &Seeded<1>, now: UnixMillis) -> Acquired {
    let [holder] = &seeded.runners;
    let acquired = seed::select_fleet_within_rotations(leases, holder, now, &seeded.fleet)
        .await
        .expect("the holder leases its held fleet");
    if acquired.kind == Kind::Fresh {
        leases
            .record_received(&acquired, now)
            .await
            .expect("the event's row opens");
    }
    let tenant_id = Uuid7::parse(&seeded.tenant).expect("a seeded tenant");
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    leases
        .issue(holder, &acquired, billed, now)
        .await
        .expect("the lease row is written")
        .expect("the claim is still held");
    acquired
}

/// The holder's fresh event resumes its live hold, and counts as the
/// holder's claim.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_claim_counts_as_the_holders() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = held(&fixtures).await;
    let before = claims(&capture, HOLDER);

    let fresh = leases_at(&fixtures.leases(), &seeded, at(ENROLLED_AT + 1)).await;

    let after = claims(&capture, HOLDER);
    assert!(fresh.resume_hold, "the fresh event resumes the hold");
    assert!(after > before, "{before} -> {after}");
    queue::clear_ready(fixtures.queue(), &seeded.fleet).await;
    fixtures.cleanup().await;
}

/// The holder's reclaims on its live hold count nothing: each starts its
/// event clean, since the first attempt may have run in the held sandbox.
/// Bounded for the module note's reason: the reclaims move the holder
/// series by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_reclaims_on_a_live_hold_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = held(&fixtures).await;
    let leases = fixtures.leases();
    let mut lease = leases_at(&leases, &seeded, at(ENROLLED_AT + 1)).await;
    let before = claims(&capture, HOLDER);

    for _reclaim in 0..CLAIMS_IN_A_ROW {
        let lapsed = lease.leased_until.saturating_add_millis(1);
        lease = leases_at(&leases, &seeded, lapsed).await;
        assert_eq!(lease.kind, Kind::Reclaim);
        assert!(!lease.resume_hold, "a reclaim builds fresh");
    }

    let moved = claims(&capture, HOLDER) - before;
    assert!(
        moved < CLAIMS_IN_A_ROW,
        "{CLAIMS_IN_A_ROW} reclaims on a live hold moved the holder series by {moved}"
    );
    queue::clear_ready(fixtures.queue(), &seeded.fleet).await;
    fixtures.cleanup().await;
}

/// The lapse is the holder's: it fell silent while its hold stood. A hold
/// past its own deadline is no hold, and a claim over it counts nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_claim_after_a_lapse_counts_as_other_after_lapse() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;
    let before = claims(&capture, OTHER_AFTER_LAPSE);

    wins(
        &leases,
        &fleet,
        &other,
        ENROLLED_AT + RUNNER_OFFLINE_AFTER_MS + 1,
    )
    .await;

    let after = claims(&capture, OTHER_AFTER_LAPSE);
    assert!(after > before, "{before} -> {after}");
    fixtures.cleanup().await;
}

/// Claims on a fleet nobody holds count under neither outcome. Bounded
/// rather than exact, for the reason the module note gives: the claims made
/// here move the counter by fewer than their own number.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_claims_on_an_unheld_fleet_count_nothing() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, runners) = seeded_parts::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let before = (
        claims(&capture, HOLDER),
        claims(&capture, OTHER_AFTER_LAPSE),
    );

    let rounds = usize::try_from(CLAIMS_IN_A_ROW).expect("sixteen fits a usize");
    for (now, runner) in (ENROLLED_AT..).zip(runners.iter().cycle()).take(rounds) {
        let claimed = leases
            .claim(&id(&fleet), runner, at(now), LEASE_TTL_MS)
            .await
            .expect("the claim answers")
            .expect("a free slot is won");
        leases
            .release(&id(&fleet), claimed.fence, at(now))
            .await
            .expect("an empty claim lets go");
    }

    let moved = (
        claims(&capture, HOLDER) - before.0,
        claims(&capture, OTHER_AFTER_LAPSE) - before.1,
    );
    assert!(
        moved.0 < CLAIMS_IN_A_ROW && moved.1 < CLAIMS_IN_A_ROW,
        "{CLAIMS_IN_A_ROW} unheld claims moved the held-claims counter by {moved:?}"
    );
    fixtures.cleanup().await;
}
