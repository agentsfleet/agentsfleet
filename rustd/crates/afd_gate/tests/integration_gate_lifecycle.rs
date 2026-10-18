//! Gate parking and durable-decision fallback over both live datastores.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_crypto::entropy::Entropy;
use afd_gate::gate::{Gates, Refused, Trigger, Verdict, Waiting};

use self::fixture::{Fixture, NOW, config, config_gates, connect_redis};

/// The rule a gate rules fleet is parked by, and the only one this suite parks on.
const APPROVE_EVERY_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#;

/// Dimensions 2.1 and 2.2 — a write fleet runs every event, and every
/// continuation of one, without a card.
///
/// This suite used to assert the opposite, and the assertion was the defect.
/// A fleet whose binding declared WRITE access parked EVERY first-encounter
/// event, and a continuation gets a fresh event identifier, so a single steer
/// raised one approval card per model turn and posted nothing. The standing
/// integration grant authorises the mint; nobody is asked per event.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_m202_001_write_fleet_and_its_continuations_pass_without_a_card() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let writing = config(true);

    // 2.1 — the first encounter.
    assert_eq!(
        gates
            .check(fixture.check("event-write-first", &writing), NOW)
            .await,
        Verdict::Pass
    );
    // 2.2 — the turns after it. Each carries its own event identifier, which is
    // exactly why the retired park re-asked: it read no `resumes_event_id` and
    // saw every turn as a first encounter.
    for turn in [
        "event-write-turn-2",
        "event-write-turn-3",
        "event-write-turn-4",
    ] {
        assert_eq!(
            gates.check(fixture.check(turn, &writing), NOW).await,
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

/// Dimension 2.3 — deleting the write park did not delete the rules path.
///
/// The boundary that survives. A workspace that asks to be consulted still is,
/// and that is the one thing this milestone must not destabilize.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_m202_001_rule_gated_fleet_still_parks() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let ruled = config_gates(APPROVE_EVERY_CHAT);

    assert_eq!(
        gates.check(fixture.check("event-ruled", &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates.check(fixture.check("event-ruled", &ruled), NOW).await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve("event-ruled", "approved").await;
    assert_eq!(
        gates.check(fixture.check("event-ruled", &ruled), NOW).await,
        Verdict::Pass
    );

    fixture.cleanup().await;
}

/// A pending gate lapses on the database's clock, and the poll that reads it
/// ends the event.
///
/// The deadline is the row's `expires_at`, stamped `now()` plus the rule's
/// timeout when the gate was raised; the poll compares it against
/// `statement_timestamp()`. Moving the deadline behind that clock is what a
/// day passing would do, and the very next poll answers Expired.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_pending_gate_past_its_deadline_on_the_database_clock_expires() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let ruled = config_gates(APPROVE_EVERY_CHAT);
    let event = "event-lapsing";
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
        Verdict::Refuse(Refused::Expired),
        "past its deadline on the database's clock, the event ends"
    );

    fixture.cleanup().await;
}

/// Dimension 2.4 — a recorded gate decides an in-flight event after the
/// fleet's config stops asking for one.
///
/// A recorded gate outranks policy, so the run stays parked until its answer
/// arrives and continues once a person gives it.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn recorded_gate_decides_after_the_config_stops_asking() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let ruled = config_gates(APPROVE_EVERY_CHAT);
    assert_eq!(
        gates
            .check(fixture.check("event-in-flight", &ruled), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );

    // A write binding with no rules asks for no gate at all. The recorded gate
    // decides anyway, so the in-flight run is not released before its answer.
    let writing = config(true);
    assert_eq!(
        gates
            .check(fixture.check("event-in-flight", &writing), NOW)
            .await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve("event-in-flight", "approved").await;
    assert_eq!(
        gates
            .check(fixture.check("event-in-flight", &writing), NOW)
            .await,
        Verdict::Pass
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn authored_rules_and_anomaly_thresholds_drive_each_first_encounter_route() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );

    let approval = config_gates(
        r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-rule-approval", &approval), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );

    let policy_kill = config_gates(
        r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-policy-kill", &policy_kill), NOW)
            .await,
        Verdict::Killed(Trigger::Policy)
    );
    fixture.activate().await;

    let anomaly = config_gates(
        r#"{"anomaly_rules":[{"pattern":"same_action","threshold_count":2,"threshold_window_s":60}]}"#,
    );
    assert_eq!(
        gates
            .check(fixture.check("event-anomaly-first", &anomaly), NOW)
            .await,
        Verdict::Pass
    );
    assert_eq!(
        gates
            .check(fixture.check("event-anomaly-second", &anomaly), NOW)
            .await,
        Verdict::Killed(Trigger::Anomaly)
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_gate_that_cannot_mint_its_identity_fails_closed() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let gates = Gates::new(fixture.database.clone(), connect_redis().await, entropy);

    assert_eq!(
        gates
            .check(
                fixture.check("event-entropy-failure", &config_gates(APPROVE_EVERY_CHAT)),
                NOW,
            )
            .await,
        Verdict::Unavailable,
        "a gate with no durable identity cannot release the event"
    );
    fixture.cleanup().await;
}
