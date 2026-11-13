//! Gate decisions preserve their durable result when a datastore is unavailable.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_gate::gate::{Claim, Evaluation, GateRef, Gates, Park, Parked, Stated, Verdict};

use self::fixture::{Fixture, NOW, RUNAWAY_AT_TWO, config, config_gates};

/// How long a parked question waits for its answer.
const REFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runaway_archives_the_fleet_even_when_the_bus_is_unreachable() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        afd_bus::test_util::unreachable(),
        Entropy::new(),
    );
    let runaway = config_gates(RUNAWAY_AT_TWO);

    gates.check(fixture.check(&mint_id(), &runaway), NOW).await;
    let verdict = gates.check(fixture.check(&mint_id(), &runaway), NOW).await;

    assert_eq!(verdict, Verdict::Archived);
    assert_eq!(
        fixture.status().await,
        afd_fleet_lifecycle::FleetStatus::Archived.as_str()
    );
    fixture.cleanup().await;
}

/// Gates over `database` and a bus that refuses every publish and subscribe.
fn gates_without_a_bus(database: afd_db::Db) -> Gates {
    Gates::new(database, afd_bus::test_util::unreachable(), Entropy::new())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_reference_whose_row_is_gone_keeps_the_event_waiting() {
    let fixture = Fixture::create().await;
    let gates = gates_without_a_bus(fixture.database.clone());

    let evaluation = gates
        .evaluate(&GateRef::new(mint_id()))
        .await
        .expect("the durable read answers");

    assert_eq!(
        evaluation,
        Evaluation::Pending,
        "no row means no deadline to lapse; waiting is the fail-safe direction"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parked_event_finds_its_gate_from_the_table() -> Result<(), &'static str> {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = gates_without_a_bus(fixture.database.clone());
    let event_id = &mint_id();
    let config = config(false);
    let scope = fixture.check(event_id, &config);

    let parked = gates
        .park(
            Park {
                fleet_id: scope.fleet_id,
                workspace_id: scope.workspace_id,
                event_id,
                stated: Stated::of(
                    "chat",
                    "user:fixture",
                    event_id.as_str(),
                    None,
                    REFERENCE_TIMEOUT,
                ),
                claim: &Claim::of(None),
            },
            NOW,
        )
        .await;
    let Parked::Awaiting(reference) = parked else {
        return Err("a parked event awaits its human");
    };

    let found = gates
        .recorded(scope.fleet_id, event_id)
        .await
        .expect("the durable read answers");

    assert_eq!(
        found.as_ref().map(GateRef::action_id),
        Some(reference.action_id()),
        "the gate's action is found from approval_gates, with no cache"
    );
    fixture.cleanup().await;
    Ok(())
}
