//! §2 against live datastores — the assignment pass end to end.
//!
//! `integration_lease_affinity.rs` proves the pieces underneath this: the
//! atomic claim, the fence, the guarded release. What is here is the pass that
//! USES them — readiness peek, candidate query, claim, then reclaim-or-fresh —
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
use afd_fleet::lease::Kind;

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
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        event_id,
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;

    let acquired = crate::seed::select_fleet(&fixtures.leases(), &runner, &fleet)
        .await
        .expect("a ready fleet holding an event is leasable");

    assert_eq!(acquired.kind, Kind::Fresh, "a first pull is not a reclaim");
    assert_eq!(acquired.fleet_id.as_str(), fleet);
    assert_eq!(
        acquired.event_id, event_id,
        "the stream entry id IS the event id"
    );
    assert_eq!(acquired.actor, ACTOR);
    assert_eq!(acquired.event_type, EVENT_TYPE);
    assert_eq!(acquired.request_json, REQUEST_JSON);
    assert_eq!(acquired.event_created_at, fixtures.enrolled_at());
    assert!(
        acquired.reused.is_none(),
        "a fresh pull carries no prior billing; the caller bills it"
    );
    assert_eq!(
        acquired.fence.as_i64(),
        1,
        "a fleet leased for the first time opens at fence one"
    );

    fixtures.cleanup().await;
}

/// An empty readiness index answers no-work without touching Postgres.
///
/// The zero-database path is the dominant steady state, and it is the reason
/// idle cost scales with runner count rather than runners × fleets. Asserted
/// through the observable outcome: a fleet that exists and holds an event is
/// NOT leased while its readiness mark is absent.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_is_discovered_without_any_notification() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;

    // A full rotation, because one poll proves nothing: since §4 it samples ONE
    // of sixteen partitions, so a single `None` is as likely to mean "wrong
    // partition" as "nothing marked". A rotation visits every partition once.
    let acquired = crate::seed::select_fleet(&fixtures.leases(), &runner, &fleet).await;

    assert!(
        acquired.is_some(),
        "the ledger probe discovers work without a notification"
    );

    fixtures.cleanup().await;
}

/// A second runner polling the same fleet gets nothing while the first holds
/// it — and reads no event doing so.
///
/// The claim precedes the read, which is what makes the loser's poll free of
/// side effects. If the order were reversed the loser would consume the entry
/// from the stream and then fail to claim, stranding it.
///
/// # Why the lapsed re-poll is a FRESH pull here and not a reclaim
///
/// Reclaim reads `runner.leases` for a still-`active` row, and this
/// suite never writes one: `select` ASSIGNS work, and the row is written by
/// the issue verb, which does not exist yet. So the fleet has no dead holder
/// to take work back from, and the pass correctly falls through to the fresh
/// read — where the entry is still on THIS consumer's pending list from the
/// first pass, and pending-first re-delivers it.
///
/// That is the pending-gate re-poll path, and asserting it is worth more than
/// asserting nothing: it proves the entry is not lost when a claim lapses
/// before a lease is issued. The reclaim-through-`select` assertion belongs
/// with the issue verb, which is what makes the precondition reachable.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_second_runner_is_refused_while_the_claim_is_live() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();

    let held = crate::seed::select_fleet(&leases, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    // A full rotation: one poll could miss the claimed fleet's partition and
    // answer `None` for a reason that has nothing to do with the claim.
    let refused = crate::seed::select_fleet(&leases, &second, &fleet).await;
    assert!(
        refused.is_none(),
        "one fleet, one holder — the second runner polls on"
    );

    // Past the holder's expiry the same fleet becomes winnable again, and the
    // SAME event comes back re-fenced rather than being lost with the claim.
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", LAPSE)
        .await;
    // Scoped like the two acquisitions above it. `seeded::<2>` leaves a second
    // fleet leasable, so an unscoped poll could win THAT one and then be
    // compared, event id and fence, against a claim it never displaced.
    let regained = crate::seed::select_fleet(&leases, &second, &fleet)
        .await
        .expect("a lapsed claim is winnable");

    assert_eq!(
        regained.event_id, held.event_id,
        "the entry survives a lapsed claim; it is re-delivered, never dropped"
    );
    assert!(
        regained.fence > held.fence,
        "the re-lease outranks the holder it displaced: {:?} vs {:?}",
        regained.fence,
        held.fence
    );
    assert_eq!(
        regained.kind,
        Kind::Fresh,
        "with no lease row written there is no dead holder to reclaim from — \
         see this test's own documentation"
    );

    fixtures.cleanup().await;
}
