//! A poll body that names held fleets: naming one wins another runner
//! nothing while its holder is live, and naming one with no work in it costs
//! the poll no Postgres.
//!
//! Each store polls a PRIVATE readiness index holding at most its own fleet,
//! so no other suite's mark is in reach and every partition is known empty
//! or known to hold that one fleet.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afd_dragonfly::ready::READY_PARTITIONS;
use afd_dragonfly::{ReadyIndex, ReadyPrefix};

use super::{at, hold, id, live};
use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// Invariant 6 through the poll body: a runner that names another live
/// runner's held fleet as its own is offered nothing, and the holder naming
/// it leases it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_naming_a_held_fleet_wins_another_runner_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    queue::clear_ready(fixtures.queue(), &fleet).await;
    let prefix = ReadyPrefix::private(&fleet);
    let index = ReadyIndex::under(fixtures.queue().clone(), prefix.clone());
    index
        .mark(&fleet)
        .await
        .expect("the private index takes a mark");
    let leases = fixtures.leases().with_ready_prefix(prefix);
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        ENROLLED_AT + SANDBOX_HOLD_IDLE_MS,
    )
    .await;
    let now = at(ENROLLED_AT + 1);
    let named = [id(&fleet)];

    let claimed = leases
        .select(&other, &named, now)
        .await
        .expect("the poll answers");
    let held = leases
        .select(&holder, &named, now)
        .await
        .expect("the poll answers");

    assert!(claimed.is_none(), "another runner gained {claimed:?}");
    assert_eq!(
        held.map(|acquired| acquired.fleet_id),
        Some(id(&fleet)),
        "the fleet was ready all along: its holder leases it"
    );
    let _cleared = index.force_clear(&fleet).await;
    fixtures.cleanup().await;
}

/// A poll naming a held fleet with no readiness mark, over an index with
/// nothing in it, answers no work without one Postgres round trip, on every
/// partition of a rotation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_idle_poll_naming_an_unmarked_held_fleet_costs_no_postgres() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures
        .leases()
        .with_ready_prefix(ReadyPrefix::private(&fleet));
    let named = [id(&fleet)];

    for _poll in 0..READY_PARTITIONS {
        let (polled, cost) = leases
            .select_measured(&holder, &named, at(ENROLLED_AT))
            .await;
        assert!(polled.expect("the poll answers").is_none());
        assert_eq!(cost.database_roundtrips, 0, "{cost:?}");
    }
    fixtures.cleanup().await;
}
