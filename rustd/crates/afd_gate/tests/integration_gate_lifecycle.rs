//! Gate parking and durable-decision fallback over both live datastores.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_crypto::entropy::Entropy;
use afd_gate::gate::{Gates, Trigger, Verdict, Waiting};

use self::fixture::{Fixture, NOW, config, config_gates, connect_redis};

/// The rule a gate rules fleet is parked by, and the only one this suite parks on.
const APPROVE_EVERY_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#;

/// The kind no daemon path raises any longer.
const RETIRED_WRITE_KIND: &str = "repository_write";

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

/// Dimension 2.4 — an event parked before this shipped is not stranded.
///
/// The row is real and so is the reference that finds it: the rules path wrote
/// both. Only `gate_kind` is restated, to the kind an earlier build raised and
/// this one never will. A recorded gate outranks policy, so the answer a person
/// gives still lands and the run still continues.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_m202_001_parked_event_still_resolves() {
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
    fixture
        .restate_kind("event-in-flight", RETIRED_WRITE_KIND)
        .await;

    // The fleet's config no longer asks for a gate at all — a write binding and
    // no rules, which after this milestone is the ordinary shape. The recorded
    // gate decides anyway, which is the property that keeps the in-flight run
    // from being released before its answer arrives.
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
