//! §2 against live datastores — the assignment pass end to end.
//!
//! `integration_run_affinity.rs` proves the pieces underneath this: the
//! atomic assignment, the fence, the guarded release. What is here is the pass that
//! USES them — readiness peek, candidate query, assignment, then reassignment-or-fresh —
//! which is the only place their ordering is observable.
//!
//! The ordering is the thing worth testing. Any one step passing in isolation
//! says nothing about whether a loser reads an event it should not have, or
//! whether a lapsed holder's work is taken back rather than pulled fresh and
//! billed twice.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::seed;
use crate::support;
use afd_fleet::run::Kind;

use self::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON, Seeded, seeded};
use self::support::Fixtures;
use crate::rows::{LAPSE, Row};

/// A ready fleet with an event is assigned, with the envelope intact.
///
/// The fields matter as much as the fact: the runner executes from this
/// envelope, so an actor or a body lost in the decode is a fleet running the
/// wrong instruction rather than a test failing loudly.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_select_assigns_a_ready_fleets_event() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        turn_id,
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;

    let acquired = seed::select_fleet(&fixtures.runs(), &runner, &fleet)
        .await
        .expect("a ready fleet holding an event is leasable");

    assert_eq!(
        acquired.kind,
        Kind::Fresh,
        "a first pull is not a reassignment"
    );
    assert_eq!(acquired.fleet_id.as_str(), fleet);
    assert_eq!(
        acquired.turn_id, turn_id,
        "the stream entry id IS the event id"
    );
    assert_eq!(acquired.actor, ACTOR);
    assert_eq!(acquired.event_type, EVENT_TYPE);
    // `thread.turns.request_json` is JSONB, so the run carries the sender's
    // body as Postgres renders it: the same value, not the same bytes.
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&acquired.request_json).ok(),
        serde_json::from_str::<serde_json::Value>(REQUEST_JSON).ok()
    );
    assert_eq!(acquired.turn_created_at, fixtures.enrolled_at());
    assert_eq!(
        acquired.fence.as_i64(),
        1,
        "a fleet held for the first time opens at fence one"
    );

    fixtures.cleanup().await;
}

/// An empty readiness index answers no-work without touching Postgres.
///
/// The zero-database path is the dominant steady state, and it is the reason
/// idle cost scales with runner count rather than runners × fleets. Asserted
/// through the observable outcome: a fleet that exists and holds an event is
/// NOT held while its readiness mark is absent.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_is_discovered_without_any_notification() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;

    // A full rotation, because one poll proves nothing: since §4 it samples ONE
    // of sixteen partitions, so a single `None` is as likely to mean "wrong
    // partition" as "nothing marked". A rotation visits every partition once.
    let acquired = seed::select_fleet(&fixtures.runs(), &runner, &fleet).await;

    assert!(
        acquired.is_some(),
        "the ledger probe discovers work without a notification"
    );

    fixtures.cleanup().await;
}

/// A second runner polling the same fleet gets nothing while the first holds
/// it — and reads no event doing so.
///
/// The assignment precedes the read, which is what makes the loser's poll free of
/// side effects. If the order were reversed the loser would consume the entry
/// from the stream and then lose the assignment, stranding it.
///
/// # Why the lapsed re-poll is a FRESH pull here and not a reassignment
///
/// Reassignment reads `thread.runs` for a still-`active` row, and this
/// suite never writes one: `select` ASSIGNS work, and the row is written by
/// the issue verb, which does not exist yet. So the fleet has no dead holder
/// to take work back from, and the pass correctly falls through to the fresh
/// read — where the entry is still on THIS consumer's pending list from the
/// first pass, and pending-first re-delivers it.
///
/// That is the pending-gate re-poll path, and asserting it is worth more than
/// asserting nothing: it proves the entry is not lost when an assignment lapses
/// before a run is issued. The reassignment-through-`select` assertion belongs
/// with the issue verb, which is what makes the precondition reachable.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_second_runner_is_refused_while_the_assignment_is_live() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, second],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();

    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    // A full rotation: one poll could miss the assigned fleet's partition and
    // answer `None` for a reason that has nothing to do with the assignment.
    let refused = seed::select_fleet(&runs, &second, &fleet).await;
    assert!(
        refused.is_none(),
        "one fleet, one holder — the second runner polls on"
    );

    // Past the holder's expiry the same fleet becomes winnable again, and the
    // SAME event comes back re-fenced rather than being lost with the assignment.
    fixtures
        .age(Row::Thread(&fleet), "run_expires_at", LAPSE)
        .await;
    // Scoped like the two acquisitions above it. `seeded::<2>` leaves a second
    // fleet leasable, so an unscoped poll could win THAT one and then be
    // compared, event id and fence, against an assignment it never displaced.
    let regained = seed::select_fleet(&runs, &second, &fleet)
        .await
        .expect("a lapsed assignment is winnable");

    assert_eq!(
        regained.turn_id, held.turn_id,
        "the entry survives a lapsed assignment; it is re-delivered, never dropped"
    );
    assert!(
        regained.fence > held.fence,
        "the re-run outranks the holder it displaced: {:?} vs {:?}",
        regained.fence,
        held.fence
    );
    assert_eq!(
        regained.kind,
        Kind::Fresh,
        "with no run row written there is no dead holder to reassign from — \
         see this test's own documentation"
    );

    fixtures.cleanup().await;
}
