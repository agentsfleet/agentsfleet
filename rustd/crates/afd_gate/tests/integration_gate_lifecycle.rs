//! Gate parking and durable-decision fallback over both live datastores.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_crypto::entropy::Entropy;
use afd_fleet_runtime::FleetConfig;
use afd_gate::gate::{Gates, Refused, Trigger, Verdict, Waiting};

use self::fixture::{Fixture, NOW, config, config_gates, connect_redis};

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn write_gate_parks_once_and_honours_each_durable_outcome() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let writing = config(true);

    assert_approved_path(&gates, &fixture, &writing).await;
    assert_denied_path(&gates, &fixture, &writing).await;
    assert_expired_path(&gates, &fixture, &writing).await;
    assert_eq!(
        gates
            .check(fixture.check("event-free", &config(false)), NOW)
            .await,
        Verdict::Pass
    );
    fixture.cleanup().await;
}

async fn assert_approved_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Await(Waiting::Pending)
    );
    fixture.resolve("event-approved", "approved").await;
    assert_eq!(
        gates
            .check(fixture.check("event-approved", writing), NOW)
            .await,
        Verdict::Pass
    );
}

async fn assert_denied_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-denied", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    fixture.resolve("event-denied", "denied").await;
    assert_eq!(
        gates
            .check(fixture.check("event-denied", writing), NOW)
            .await,
        Verdict::Refuse(Refused::Denied)
    );
}

async fn assert_expired_path(gates: &Gates, fixture: &Fixture, writing: &FleetConfig) {
    assert_eq!(
        gates
            .check(fixture.check("event-expired", writing), NOW)
            .await,
        Verdict::Await(Waiting::Parked)
    );
    assert_eq!(
        gates
            .check(
                fixture.check("event-expired", writing),
                NOW.saturating_add_millis(3_600_001),
            )
            .await,
        Verdict::Refuse(Refused::Expired)
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
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
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_gate_that_cannot_mint_its_identity_fails_closed() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let gates = Gates::new(fixture.database.clone(), connect_redis().await, entropy);

    assert_eq!(
        gates
            .check(fixture.check("event-entropy-failure", &config(true)), NOW)
            .await,
        Verdict::Unavailable,
        "a gate with no durable identity cannot release the event"
    );
    fixture.cleanup().await;
}
