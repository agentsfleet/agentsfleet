//! §2 against a live datastore — the atomic claim, and what racing it proves.
//!
//! Invariant 1 is "at most one active lease per fleet", and the whole of it
//! rests on one conditional UPSERT. `dispatch/write_rust.md` asks for a
//! deterministic contention test rather than a happy-path asynchronous one, so
//! the race below is not "spawn two tasks and hope they overlap": every runner
//! contends for the SAME slot at the same instant, and the assertion is on the
//! count of winners, which is exact whatever the interleaving turns out to be.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;

/// The fleet every test here contends for.
const FLEET: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ec001";

/// Its workspace.
const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ec002";

/// Its billing tenant.
const TENANT: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ec003";

/// How many runners pile onto one slot.
///
/// Four rather than two: with two, a claim that was accidentally
/// last-write-wins still produces one row and the test passes for the wrong
/// reason. With four, a broken claim shows up as a fence that skipped values or
/// as more than one winner.
const CONTENDERS: usize = 4;

/// Enrols `N` runners against `fixtures`, returning their ids.
///
/// Fixed-size so callers DESTRUCTURE rather than index: `let [a, b] = …` cannot
/// panic, where `runners[1]` can, and a contention test that panics on its own
/// scaffolding reports the wrong failure.
async fn enrol<const N: usize>(fixtures: &Fixtures) -> [Uuid7; N] {
    let mut runners = Vec::with_capacity(N);
    for _ in 0..N {
        let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress, 1);
        let enrolled = fixtures
            .runners()
            .register(&request, UnixMillis::from_millis(ENROLLED_AT))
            .await
            .expect("enrolment must succeed");
        runners.push(enrolled.runner_id);
    }
    runners
        .try_into()
        .expect("N enrolments produce exactly N identifiers")
}

/// Dimension 2.2 — two runners race one fleet, and exactly one leases.
///
/// The losers must lose CLEANLY: `None` is the documented `.taken` verdict, not
/// an error, because a loser that saw a failure would back off instead of
/// moving to the next candidate.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_lease_affinity_race() {
    let fixtures = Fixtures::create_with_queue().await;
    fixtures
        .seed_fleet(FLEET, WORKSPACE, TENANT, ENROLLED_AT)
        .await;
    let runners: [Uuid7; CONTENDERS] = enrol(&fixtures).await;
    let fleet = Uuid7::parse(FLEET).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    // Every contender claims the same slot at the same instant. Awaited
    // together rather than in sequence so the datastore, not this loop, decides
    // the order they arrive in.
    let attempts = runners
        .iter()
        .map(|runner| leases.claim(&fleet, runner, now, LEASE_TTL_MS));
    let outcomes = futures_util::future::join_all(attempts).await;

    let won: Vec<_> = outcomes
        .into_iter()
        .filter_map(|outcome| outcome.expect("a claim attempt must not fault"))
        .collect();

    assert_eq!(
        won.len(),
        1,
        "exactly one runner may hold a fleet's slot; the losers read as taken, not as errors"
    );
    assert_eq!(
        fixtures.affinity_column(FLEET, "fencing_seq").await,
        Some("1".to_owned()),
        "a slot claimed for the first time opens at fence one"
    );
    let winner = won.first().expect("the count above proved there is one");
    assert_eq!(
        winner.fence.as_i64(),
        1,
        "the winner carries the fence the row now holds"
    );

    fixtures.cleanup().await;
}

/// A claim taken after the prior one lapsed carries a STRICTLY higher fence.
///
/// Invariant 2's first half. Without it a reclaimed lease would be
/// indistinguishable from the dead holder it displaced, and §3's report could
/// not tell a stale writer from the live one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_reclaim_bumps_fence() {
    let fixtures = Fixtures::create_with_queue().await;
    fixtures
        .seed_fleet(FLEET, WORKSPACE, TENANT, ENROLLED_AT)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures).await;
    let fleet = Uuid7::parse(FLEET).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let first = leases
        .claim(&fleet, &first_runner, now, LEASE_TTL_MS)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");

    // A live holder blocks the slot, whoever asks.
    let blocked = leases
        .claim(&fleet, &second_runner, now, LEASE_TTL_MS)
        .await
        .expect("a blocked claim is not a fault");
    assert!(
        blocked.is_none(),
        "a live claim must not be displaceable before it lapses"
    );

    // Past the holder's expiry, the slot is winnable again — and the winner's
    // token outranks the holder it displaced.
    let lapsed = first.leased_until.saturating_add_millis(1);
    let reclaimed = leases
        .claim(&fleet, &second_runner, lapsed, LEASE_TTL_MS)
        .await
        .expect("the reclaim must not fault")
        .expect("a lapsed slot is winnable");

    assert!(
        reclaimed.fence > first.fence,
        "a reclaim's token must strictly exceed the one it displaced: {:?} vs {:?}",
        reclaimed.fence,
        first.fence
    );

    fixtures.cleanup().await;
}

/// Release is fence-guarded, so a superseded holder cannot free the live slot.
///
/// This is the failure the guard exists for: without it a dead runner's late
/// release would hand a fleet that is actively being worked to a second runner.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_superseded_holder_cannot_release_the_live_slot() {
    let fixtures = Fixtures::create_with_queue().await;
    fixtures
        .seed_fleet(FLEET, WORKSPACE, TENANT, ENROLLED_AT)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures).await;
    let fleet = Uuid7::parse(FLEET).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let stale = leases
        .claim(&fleet, &first_runner, now, LEASE_TTL_MS)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");
    let lapsed = stale.leased_until.saturating_add_millis(1);
    let live = leases
        .claim(&fleet, &second_runner, lapsed, LEASE_TTL_MS)
        .await
        .expect("the reclaim must not fault")
        .expect("a lapsed slot is winnable");

    // The displaced holder tries to tidy up, carrying its own stale token.
    leases
        .release(&fleet, stale.fence, lapsed)
        .await
        .expect("a guarded release is a no-op, never a fault");

    assert_eq!(
        fixtures.affinity_column(FLEET, "leased_until").await,
        Some(live.leased_until.as_millis().to_string()),
        "the live holder's claim must survive a superseded holder's release"
    );

    // The live holder's own release does land, because its token still matches.
    leases
        .release(&fleet, live.fence, lapsed)
        .await
        .expect("the current holder's release must run");
    assert_eq!(
        fixtures.affinity_column(FLEET, "leased_until").await,
        Some(lapsed.as_millis().to_string()),
        "the current holder frees the slot it actually holds"
    );

    fixtures.cleanup().await;
}

/// A fresh lease resets the metering cursor; the claim itself preserves it.
///
/// The pairing is the point. `RESET_AFFINITY_METERS` is a separate statement
/// precisely so that a RECLAIM can skip it and meter forward from the dead
/// holder's progress — if the claim cleared the cursor itself, a re-leased run
/// would be billed from zero for work already charged.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_reclaim_preserves_the_meter_a_fresh_lease_resets() {
    let fixtures = Fixtures::create_with_queue().await;
    fixtures
        .seed_fleet(FLEET, WORKSPACE, TENANT, ENROLLED_AT)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures).await;
    let fleet = Uuid7::parse(FLEET).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let first = leases
        .claim(&fleet, &first_runner, now, LEASE_TTL_MS)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");

    // Stand in for a run that metered some tokens before its holder died.
    fixtures.set_metered_input(FLEET, METERED).await;

    let lapsed = first.leased_until.saturating_add_millis(1);
    leases
        .claim(&fleet, &second_runner, lapsed, LEASE_TTL_MS)
        .await
        .expect("the reclaim must not fault")
        .expect("a lapsed slot is winnable");

    assert_eq!(
        fixtures
            .affinity_column(FLEET, "metered_input_tokens")
            .await,
        Some(METERED.to_string()),
        "a reclaim meters FORWARD, so the claim must not clear the cursor"
    );

    // A fresh lease is the one that starts the slice over.
    leases
        .reset_meters(&fleet, lapsed)
        .await
        .expect("the reset must run");
    assert_eq!(
        fixtures
            .affinity_column(FLEET, "metered_input_tokens")
            .await,
        Some("0".to_owned()),
        "a fresh lease meters from zero"
    );

    fixtures.cleanup().await;
}

/// Tokens a dead holder is standing in for.
///
/// Any non-zero value proves the property; named so the two assertions above
/// read against one another rather than against a bare literal (RULE UFS).
const METERED: i64 = 1_234;
