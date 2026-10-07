//! Who won a claim on a held fleet, as the held-claims counter records it.
//!
//! The counter is process-wide and the suite's tests run in parallel, so a
//! claim here is read as a movement of its own series rather than an exact
//! total: other held-sandbox tests claim held fleets at the same time.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL_MS, RUNNER_OFFLINE_AFTER_MS, SANDBOX_HOLD_IDLE_MS};
use afd_fleet::lease::Leases;
use afd_observability::test_util::Capture;

use super::{at, hold, id, live};
use crate::requests::ENROLLED_AT;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// The family a won claim on a held fleet counts under.
const HELD_CLAIMS: &str = "agentsfleet_lease_held_claims_total";

/// Its one label, and the two outcomes it names.
const OUTCOME: &str = "outcome";
const HOLDER: &str = "holder";
const OTHER_AFTER_LAPSE: &str = "other_after_lapse";

/// Unheld claims made in a row. Well past every held claim a neighbour in
/// this suite makes, so a counter moved by unheld claims cannot pass for them.
const UNHELD_CLAIMS: u64 = 16;

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

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_holders_claim_counts_as_the_holders() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        ENROLLED_AT + SANDBOX_HOLD_IDLE_MS,
    )
    .await;
    let before = claims(&capture, HOLDER);

    wins(&leases, &fleet, &holder, ENROLLED_AT + 1).await;

    let after = claims(&capture, HOLDER);
    assert!(after > before, "{before} -> {after}");
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
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        ENROLLED_AT + SANDBOX_HOLD_IDLE_MS,
    )
    .await;
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

    let rounds = usize::try_from(UNHELD_CLAIMS).expect("sixteen fits a usize");
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
        moved.0 < UNHELD_CLAIMS && moved.1 < UNHELD_CLAIMS,
        "{UNHELD_CLAIMS} unheld claims moved the held-claims counter by {moved:?}"
    );
    fixtures.cleanup().await;
}
