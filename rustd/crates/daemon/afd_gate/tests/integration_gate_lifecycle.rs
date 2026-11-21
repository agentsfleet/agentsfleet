//! Gate parking and durable-decision fallback over both live datastores.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_gate::gate::{Gates, Verdict, Waiting};

use self::fixture::{Fixture, NOW, bus, config, config_gates};

/// The rule a gate rules fleet is parked by, and the only one this suite parks on.
const APPROVE_EVERY_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","gate_kind":"deploy","blast_radius":"production"}]}"#;

/// A write fleet runs every event, and every
/// continuation of one, without a card.
///
/// This suite used to assert the opposite, and the assertion was the defect.
/// A fleet whose binding declared WRITE access parked EVERY first-encounter
/// event, and a continuation gets a fresh event identifier, so a single steer
/// raised one approval card per model turn and posted nothing. The standing
/// integration grant authorises the mint; nobody is asked per event.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_m202_001_write_fleet_and_its_continuations_pass_without_a_card() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());
    let writing = config(true);

    // 2.1 — the first encounter.
    assert_eq!(
        gates.check(fixture.check(&mint_id(), &writing), NOW).await,
        Verdict::Pass
    );
    // 2.2 — the turns after it. Each carries its own event identifier, which is
    // exactly why the retired park re-asked: it read no `resumes_turn_id` and
    // saw every turn as a first encounter.
    for turn in [mint_id(), mint_id(), mint_id()] {
        assert_eq!(
            gates.check(fixture.check(&turn, &writing), NOW).await,
            Verdict::Pass,
            "{turn} raised a card"
        );
    }

    assert_eq!(
        fixture.card_count().await,
        0,
        "a write fleet raised an approval card"
    );
    fixture.cleanup().await;
}

/// Deleting the write park did not delete the rules path.
///
/// The boundary that survives. A workspace that asks to be consulted still is,
/// and that is the one thing this milestone must not destabilize.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_m202_001_rule_gated_fleet_still_parks() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());
    let ruled = config_gates(APPROVE_EVERY_CHAT);
    let event = mint_id();

    assert_eq!(
        gates.check(fixture.check(&event, &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates.check(fixture.check(&event, &ruled), NOW).await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve(&event, "approved").await;
    assert_eq!(
        gates.check(fixture.check(&event, &ruled), NOW).await,
        Verdict::Pass
    );

    fixture.cleanup().await;
}

/// A pending gate past its deadline keeps its event waiting: nothing expires
/// a gate, and a late answer still decides it (`M253_003` §2).
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_pending_gate_past_its_deadline_keeps_its_event_waiting() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());
    let ruled = config_gates(APPROVE_EVERY_CHAT);
    let event = &mint_id();
    assert_eq!(
        gates.check(fixture.check(event, &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates.check(fixture.check(event, &ruled), NOW).await,
        Verdict::Await(Waiting::Pending),
        "inside its window the gate waits"
    );

    fixture.lapse(event).await;
    assert_eq!(
        gates.check(fixture.check(event, &ruled), NOW).await,
        Verdict::Await(Waiting::Pending),
        "past its deadline the event still waits for a person"
    );

    fixture.cleanup().await;
}

/// A recorded gate decides an in-flight event after the
/// fleet's config stops asking for one.
///
/// A recorded gate outranks policy, so the run stays parked until its answer
/// arrives and continues once a person gives it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn recorded_gate_decides_after_the_config_stops_asking() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());
    let ruled = config_gates(APPROVE_EVERY_CHAT);
    let event = mint_id();
    assert_eq!(
        gates.check(fixture.check(&event, &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );

    // A write binding with no rules asks for no gate at all. The recorded gate
    // decides anyway, so the in-flight run is not released before its answer.
    let writing = config(true);
    assert_eq!(
        gates.check(fixture.check(&event, &writing), NOW).await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve(&event, "approved").await;
    assert_eq!(
        gates.check(fixture.check(&event, &writing), NOW).await,
        Verdict::Pass
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_authored_rule_parks_its_first_encounter() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());

    let approval = config_gates(
        r#"{"rules":[{"tool":"chat","action":"user:fixture","gate_kind":"deploy","blast_radius":"production"}]}"#,
    );
    assert_eq!(
        gates.check(fixture.check(&mint_id(), &approval), NOW).await,
        Verdict::Await(Waiting::Parked)
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_gate_that_cannot_mint_its_identity_fails_closed() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let gates = Gates::new(fixture.database.clone(), bus().await, entropy);

    assert_eq!(
        gates
            .check(
                fixture.check(&mint_id(), &config_gates(APPROVE_EVERY_CHAT)),
                NOW,
            )
            .await,
        Verdict::Unavailable,
        "a gate with no durable identity cannot release the event"
    );
    fixture.cleanup().await;
}
