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

use crate::requests;
use crate::support;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use self::requests::{enrolment_tagged, placement_tag};
use afd_db::test_util::mint_id;

use self::support::Fixtures;
use crate::rows::{LAPSE, Row};

/// The fleet one test contends for, and the rows it hangs from.
///
/// Minted per test rather than three `const`s shared by all four. These are
/// PRIMARY KEYS, and with every test in the lane sharing one database a fixed
/// one is a unique-violation the moment a second test seeds its own fleet —
/// see [`afd_db::test_util::mint_id`].
struct Ids {
    fleet: String,
    workspace: String,
    tenant: String,
}

impl Ids {
    /// Three identifiers no other test in this lane can name.
    fn mint() -> Self {
        Self {
            fleet: mint_id(),
            workspace: mint_id(),
            tenant: mint_id(),
        }
    }
}

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
async fn enrol<const N: usize>(fixtures: &Fixtures, tag: &str) -> [Uuid7; N] {
    let mut runners = Vec::with_capacity(N);
    for _ in 0..N {
        let request = enrolment_tagged(
            SandboxTier::LandlockFull,
            NetworkPolicy::AllowListEgress,
            1,
            tag,
        );
        let enrolled = fixtures
            .runners()
            .register(&request, fixtures.enrolled_at())
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
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let runners: [Uuid7; CONTENDERS] = enrol(&fixtures, &tag).await;
    let fleet = Uuid7::parse(&ids.fleet).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();

    // Every contender claims the same slot at the same instant. Awaited
    // together rather than in sequence so the datastore, not this loop, decides
    // the order they arrive in.
    let attempts = runners
        .iter()
        .map(|runner| leases.claim(&fleet, runner, LEASE_TTL));
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
        fixtures.text(Row::Slot(&ids.fleet), "fencing_token").await,
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
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures, &tag).await;
    let fleet = Uuid7::parse(&ids.fleet).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();

    let first = leases
        .claim(&fleet, &first_runner, LEASE_TTL)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");

    // A live holder blocks the slot, whoever asks.
    let blocked = leases
        .claim(&fleet, &second_runner, LEASE_TTL)
        .await
        .expect("a blocked claim is not a fault");
    assert!(
        blocked.is_none(),
        "a live claim must not be displaceable before it lapses"
    );

    // Past the holder's expiry, the slot is winnable again — and the winner's
    // token outranks the holder it displaced.
    fixtures
        .age(Row::Slot(&ids.fleet), "lease_expires_at", LAPSE)
        .await;
    let reclaimed = leases
        .claim(&fleet, &second_runner, LEASE_TTL)
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
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures, &tag).await;
    let fleet = Uuid7::parse(&ids.fleet).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();

    let stale = leases
        .claim(&fleet, &first_runner, LEASE_TTL)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");
    fixtures
        .age(Row::Slot(&ids.fleet), "lease_expires_at", LAPSE)
        .await;
    let live = leases
        .claim(&fleet, &second_runner, LEASE_TTL)
        .await
        .expect("the reclaim must not fault")
        .expect("a lapsed slot is winnable");

    // The displaced holder tries to tidy up, carrying its own stale token.
    leases
        .release(&fleet, stale.fence)
        .await
        .expect("a guarded release is a no-op, never a fault");

    assert_eq!(
        fixtures
            .instant(
                Row::Slot(&ids.fleet),
                crate::lease_reads::COLUMN_LEASED_UNTIL
            )
            .await,
        Some(live.leased_until),
        "the live holder's claim must survive a superseded holder's release"
    );

    // The live holder's own release does land, because its token still matches.
    leases
        .release(&fleet, live.fence)
        .await
        .expect("the current holder's release must run");
    assert!(
        fixtures.slot_is_free(&ids.fleet).await,
        "the current holder frees the slot it actually holds"
    );

    fixtures.cleanup().await;
}

/// The claim itself preserves the metering cursor, even when it displaces a
/// lapsed holder.
///
/// The reset belongs to the lease insert, and only a FRESH lease arms it —
/// `integration_lease_issue.rs` proves both halves of that. If the claim
/// cleared the cursor itself, a re-leased run would be billed from zero for
/// work already charged.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_reclaiming_claim_preserves_the_meter() {
    let fixtures = Fixtures::create_with_queue().await;
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures, &tag).await;
    let fleet = Uuid7::parse(&ids.fleet).expect("the fixture id is a v7 spelling");
    let leases = fixtures.leases();

    leases
        .claim(&fleet, &first_runner, LEASE_TTL)
        .await
        .expect("the first claim must not fault")
        .expect("an unclaimed slot is winnable");

    // Stand in for a run that metered some tokens before its holder died.
    fixtures.set_metered_input(&ids.fleet, METERED).await;

    fixtures
        .age(Row::Slot(&ids.fleet), "lease_expires_at", LAPSE)
        .await;
    leases
        .claim(&fleet, &second_runner, LEASE_TTL)
        .await
        .expect("the reclaim must not fault")
        .expect("a lapsed slot is winnable");

    assert_eq!(
        fixtures
            .text(
                Row::Slot(&ids.fleet),
                crate::lease_reads::COLUMN_METERED_INPUT
            )
            .await,
        Some(METERED.to_string()),
        "a reclaim meters FORWARD, so the claim must not clear the cursor"
    );

    fixtures.cleanup().await;
}

/// Tokens a dead holder is standing in for.
///
/// Any non-zero value proves the property; named so the two assertions above
/// read against one another rather than against a bare literal (RULE UFS).
const METERED: i64 = 1_234;
