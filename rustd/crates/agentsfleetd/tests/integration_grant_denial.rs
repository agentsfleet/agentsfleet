//! A connected workspace still needs fleet approval; denial must drain the queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::e2e::{GOOD_KEK, Scenario, scenario};
use crate::reads::event_column;
use crate::wire::{capable_beat, field, json, post};
use afd_approval::{Decision, Inbox};
use afd_core::id::Uuid7;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_fleet::lease::{Leases, runner_consumer};
use afd_redis::FleetStreams;
use afd_vault::{SecretBody, SecretName, Vault};
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::sync::Arc;

const SERVICE: &str = "github";
const LEASES: &str = "/v1/runners/me/leases";
const LABEL: &str = "COALESCE(failure_label, '')";

async fn connect_and_declare(run: &Scenario) {
    let vault = Vault::new(
        run.booted.database.clone(),
        Arc::new(Kek::from_hex(GOOD_KEK).expect("fixture key")),
        Entropy::new(),
    );
    let raw = serde_json::value::RawValue::from_string(
        json!({"integration": SERVICE, "app_id": "7", "installation_id": "42"}).to_string(),
    )
    .expect("handle JSON");
    vault
        .create(
            &Uuid7::parse(&run.workspace).expect("workspace"),
            &SecretName::parse(SERVICE).expect("secret name"),
            &SecretBody::parse(&raw).expect("handle body"),
            run.seeded_at,
        )
        .await
        .expect("existing connection");
    sqlx::query("UPDATE core.fleets SET config_json = jsonb_set(config_json, '{x-agentsfleet,credentials}', $2::jsonb) WHERE id = $1::uuid")
        .bind(&run.fleet).bind(json!([SERVICE]).to_string())
        .execute(&mut *run.booted.database.acquire().await.expect("connection"))
        .await.expect("declare credential");
    // Prefer this fixture's fleet over unrelated ready marks left by other suites.
    Leases::new(
        run.booted.database.clone(),
        run.booted.queue.clone(),
        Entropy::new(),
    )
    .claim(
        &Uuid7::parse(&run.fleet).expect("fleet"),
        &run.runner_id,
        run.seeded_at,
        0,
    )
    .await
    .expect("initial affinity claim")
    .expect("new fleet is unclaimed");
}

async fn poll(http: &reqwest::Client, run: &Scenario) {
    // A no-work poll retains its affinity claim until expiry. Move that deadline
    // into the past so this test reaches redelivery without a wall-clock sleep.
    // Redis remains untouched: a missing acknowledgment must still be visible.
    sqlx::query("UPDATE fleet.runner_affinity SET leased_until = $2 WHERE fleet_id = $1::uuid")
        .bind(&run.fleet)
        .bind(afd_core::clock::now().as_millis() - 1)
        .execute(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("expire the previous no-work claim");
    let response = post(http, run, LEASES, &json!({})).await;
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(field(&json(response).await, "lease"), &json!(null));
}

async fn deny(run: &Scenario) {
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action_id FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
    )
    .bind(&run.fleet)
    .fetch_all(&mut *run.booted.database.acquire().await.expect("connection"))
    .await
    .expect("actionable cards");
    assert_eq!(
        actions.len(),
        1,
        "a connected workspace gets one fleet approval card"
    );
    Inbox::new(run.booted.database.clone(), run.booted.queue.clone())
        .resolve(
            actions.first().expect("card"),
            Decision::Denied,
            "fixture",
            "",
            Some(&run.fleet),
            afd_core::clock::now(),
        )
        .await
        .expect("denial");
}

async fn refusal_drains(preended: bool) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    connect_and_declare(&run).await;
    let http = reqwest::Client::new();
    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200);
    poll(&http, &run).await;
    poll(&http, &run).await;
    deny(&run).await;
    if preended {
        // The durable half succeeded before a crash or a failed Redis acknowledgment.
        Leases::new(
            run.booted.database.clone(),
            run.booted.queue.clone(),
            Entropy::new(),
        )
        .block(
            &Uuid7::parse(&run.fleet).expect("fleet"),
            &run.event_id,
            afd_fleet::lease::admit::Refusal {
                label: afd_core::event::label::GRANT_DENIED,
                detail: "",
            },
            afd_core::clock::now(),
        )
        .await
        .expect("terminal write before retry");
    }
    poll(&http, &run).await;
    let first_label = event_column(&run, &run.event_id, LABEL).await;
    let pending = FleetStreams::new(run.booted.queue.clone())
        .read_pending(&run.fleet, &runner_consumer())
        .await
        .expect("pending read");
    let second = run.enqueue_event("chat").await;
    poll(&http, &run).await;
    let second_label = event_column(&run, &second, LABEL).await;
    supervisor.shutdown().await;
    run.cleanup().await;
    assert_eq!(
        first_label.as_deref(),
        Some(afd_core::event::label::GRANT_DENIED)
    );
    assert!(
        pending.is_none(),
        "a terminal refusal must acknowledge its pending entry"
    );
    assert_eq!(
        second_label.as_deref(),
        Some(afd_core::event::label::GRANT_DENIED),
        "the next event must be reached after the denied one"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_grant_does_not_block_later_events() {
    refusal_drains(false).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_terminal_refusal_redelivery_retries_acknowledgment() {
    refusal_drains(true).await;
}
