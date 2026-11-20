//! The atomic assignment, and what racing it proves.
//!
//! Invariant 1 is "at most one active run per fleet", and the whole of it
//! rests on one conditional UPSERT. `dispatch/write_rust.md` asks for a
//! deterministic contention test rather than a happy-path asynchronous one, so
//! the race below is not "spawn two tasks and hope they overlap": every runner
//! contends for the SAME thread at the same instant, and the assertion is on the
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
use af_core::id::Uuid7;
use af_core::timing::RUN_TTL;
use af_wire::runner::{NetworkPolicy, SandboxTier};

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
    fleet: Uuid7,
    workspace: Uuid7,
    tenant: Uuid7,
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

/// How many runners pile onto one thread.
///
/// Four rather than two: with two, an assignment that was accidentally
/// last-write-wins still produces one row and the test passes for the wrong
/// reason. With four, a broken assignment shows up as a fence that skipped values or
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
            .register(
                &request,
                "fixture:platform-operator",
                fixtures.enrolled_at(),
            )
            .await
            .expect("enrolment must succeed");
        runners.push(enrolled.runner_id);
    }
    runners
        .try_into()
        .expect("N enrolments produce exactly N identifiers")
}

/// Two runners race one fleet, and exactly one runs.
///
/// The losers must lose CLEANLY: `None` is the documented `.taken` verdict, not
/// an error, because a loser that saw a failure would back off instead of
/// moving to the next candidate.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_run_affinity_race() {
    let fixtures = Fixtures::create().await;
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let runners: [Uuid7; CONTENDERS] = enrol(&fixtures, &tag).await;
    let fleet = ids.fleet.clone();
    let runs = fixtures.runs();

    // Every contender asks for the same thread at the same instant. Awaited
    // together rather than in sequence so the datastore, not this loop, decides
    // the order they arrive in.
    let attempts = runners
        .iter()
        .map(|runner| runs.assign(&fleet, runner, RUN_TTL));
    let outcomes = futures_util::future::join_all(attempts).await;

    let won: Vec<_> = outcomes
        .into_iter()
        .filter_map(|outcome| outcome.expect("an assignment attempt must not fault"))
        .collect();

    assert_eq!(
        won.len(),
        1,
        "exactly one runner may hold a fleet's thread; the losers read as taken, not as errors"
    );
    assert_eq!(
        fixtures
            .text(Row::Thread(&ids.fleet), "fencing_token")
            .await,
        Some("1".to_owned()),
        "a thread assigned for the first time opens at fence one"
    );
    let winner = won.first().expect("the count above proved there is one");
    assert_eq!(
        winner.fence.as_i64(),
        1,
        "the winner carries the fence the row now holds"
    );

    fixtures.cleanup().await;
}

/// An assignment taken after the prior one lapsed carries a STRICTLY higher fence.
///
/// Invariant 2's first half. Without it a reassigned run would be
/// indistinguishable from the dead holder it displaced, and the report could
/// not tell a stale writer from the live one.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_reassignment_bumps_fence() {
    let fixtures = Fixtures::create().await;
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures, &tag).await;
    let fleet = ids.fleet.clone();
    let runs = fixtures.runs();

    let first = runs
        .assign(&fleet, &first_runner, RUN_TTL)
        .await
        .expect("the first assignment must not fault")
        .expect("an unassigned thread is winnable");

    // A live holder blocks the thread, whoever asks.
    let blocked = runs
        .assign(&fleet, &second_runner, RUN_TTL)
        .await
        .expect("a blocked assignment is not a fault");
    assert!(
        blocked.is_none(),
        "a live assignment must not be displaceable before it lapses"
    );

    // Past the holder's expiry, the thread is winnable again — and the winner's
    // token outranks the holder it displaced.
    fixtures
        .age(Row::Thread(&ids.fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = runs
        .assign(&fleet, &second_runner, RUN_TTL)
        .await
        .expect("the reassignment must not fault")
        .expect("a lapsed thread is winnable");

    assert!(
        reassigned.fence > first.fence,
        "a reassignment's token must strictly exceed the one it displaced: {:?} vs {:?}",
        reassigned.fence,
        first.fence
    );

    fixtures.cleanup().await;
}

/// Release is fence-guarded, so a superseded holder cannot free the live thread.
///
/// This is the failure the guard exists for: without it a dead runner's late
/// release would hand a fleet that is actively being worked to a second runner.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_superseded_holder_cannot_release_the_live_thread() {
    let fixtures = Fixtures::create().await;
    let ids = Ids::mint();
    let tag = placement_tag(&ids.fleet);
    fixtures
        .seed_fleet(&ids.fleet, &ids.workspace, &ids.tenant, &tag)
        .await;
    let [first_runner, second_runner] = enrol(&fixtures, &tag).await;
    let fleet = ids.fleet.clone();
    let runs = fixtures.runs();

    let stale = runs
        .assign(&fleet, &first_runner, RUN_TTL)
        .await
        .expect("the first assignment must not fault")
        .expect("an unassigned thread is winnable");
    fixtures
        .age(Row::Thread(&ids.fleet), "run_expires_at", LAPSE)
        .await;
    let live = runs
        .assign(&fleet, &second_runner, RUN_TTL)
        .await
        .expect("the reassignment must not fault")
        .expect("a lapsed thread is winnable");

    // The displaced holder tries to tidy up, carrying its own stale token.
    runs.release(&fleet, stale.fence)
        .await
        .expect("a guarded release is a no-op, never a fault");

    assert_eq!(
        fixtures
            .instant(Row::Thread(&ids.fleet), crate::run_reads::COLUMN_HELD_UNTIL)
            .await,
        Some(live.held_until),
        "the live holder's assignment must survive a superseded holder's release"
    );

    // The live holder's own release does land, because its token still matches.
    runs.release(&fleet, live.fence)
        .await
        .expect("the current holder's release must test_run");
    assert!(
        fixtures.thread_is_free(&ids.fleet).await,
        "the current holder frees the thread it actually holds"
    );

    fixtures.cleanup().await;
}
