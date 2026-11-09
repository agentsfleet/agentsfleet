//! A connected workspace still needs fleet approval; denial must drain the queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::e2e::{GOOD_KEK, Scenario, scenario};
use crate::reads::event_column;
use crate::wire::capable_beat;
use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::event::EventType;
use afd_approval::{Decision, Inbox};
use afd_fleet::lease::Leases;
use afd_vault::{SecretBody, SecretName, Vault};
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::sync::Arc;

const SERVICE: &str = "github";
const LABEL: &str = "COALESCE(failure_reason, '')";

async fn connect_and_declare(run: &Scenario) {
    connect_credential(run).await;
    // Declared the way a person declares it: an Apply selecting a setup that
    // names the credential. The lease reads the selected revision, never a
    // copy on the fleet row.
    let mut connection = run.booted.database.acquire().await.expect("connection");
    let mut config: serde_json::Value = sqlx::query_scalar::<_, String>(
        "SELECT (r.definition -> 'config')::text FROM fleet.fleets f \
         JOIN fleet.revisions r ON r.id = f.head_revision_id WHERE f.id = $1",
    )
    .bind(&run.fleet)
    .fetch_one(&mut *connection)
    .await
    .map(|text| serde_json::from_str(&text).expect("stored config JSON"))
    .expect("the fleet's config reads");
    config
        .get_mut("x-agentsfleet")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the stored config carries its runtime block")
        .insert("credentials".to_owned(), json!([SERVICE]));
    let definition =
        crate::e2e_native::definition(&config.to_string(), crate::e2e_seed::FIXTURE_INSTRUCTIONS);
    crate::e2e_native::reselect(&mut connection, &run.fleet, &definition).await;
    drop(connection);
    // Prefer this fixture's fleet over unrelated ready marks left by other suites.
    Leases::new(
        run.booted.database.clone(),
        run.booted.queue.clone(),
        afd_bus::dragonfly::publisher(run.booted.queue.clone()),
        Entropy::new(),
    )
    .claim(&run.fleet, &run.runner_id, std::time::Duration::ZERO)
    .await
    .expect("initial affinity claim")
    .expect("new fleet is unclaimed");
}

/// How many approval cards this fixture's fleet has raised.
async fn gate_cards(run: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1")
        .bind(&run.fleet)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the gate count must run")
}

/// Wait for the durable outcome while refusing any offer for this fleet.
async fn wait_without_offer<F, Fut>(run: &Scenario, awaited: &str, mut reached: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let observed = crate::wire::wait_without_offer(run, || {
        let reached = reached();
        async {
        sqlx::query(
            "UPDATE runner.fleet_slots SET lease_expires_at = clock_timestamp() - interval '1 millisecond' \
             WHERE fleet_id = $1",
        )
        .bind(&run.fleet)
        .execute(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("expire the previous no-work claim");
        afd_bus::wake::publish(
            &afd_bus::dragonfly::publisher(run.booted.queue.clone()),
            run.fleet.clone(),
        ).await;
        reached.await
        }
    })
    .await;
    assert!(observed, "{awaited}; inspect AFD_TEST_LOG=1");
}

async fn deny(run: &Scenario) {
    let actions: Vec<Uuid7> =
        sqlx::query_scalar("SELECT action_id FROM fleet.approval_gates WHERE fleet_id = $1")
            .bind(&run.fleet)
            .fetch_all(&mut *run.booted.database.acquire().await.expect("connection"))
            .await
            .expect("actionable cards");
    assert_eq!(
        actions.len(),
        1,
        "a connected workspace gets one fleet approval card"
    );
    Inbox::new(
        run.booted.database.clone(),
        afd_bus::dragonfly::publisher(run.booted.queue.clone()),
        afd_admission::Admissions::for_tests(run.booted.database.clone(), run.booted.queue.clone()),
    )
    .resolve(
        actions.first().expect("card").as_str(),
        Decision::Denied,
        "fixture",
        "",
        Some(run.fleet.as_str()),
    )
    .await
    .expect("denial");
}

async fn refusal_drains(preended: bool) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    connect_and_declare(&run).await;
    let beat = crate::wire::heartbeat(&run, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );
    wait_without_offer(&run, "the gate never saw this fleet", || async {
        gate_cards(&run).await == 1
    })
    .await;
    assert_eq!(
        gate_cards(&run).await,
        1,
        "the session claim raised its approval card"
    );
    deny(&run).await;
    preend(&run, preended).await;
    wait_without_offer(&run, "the denied admission never settled", || async {
        sqlx::query_scalar::<_, bool>(
            "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1",
        )
        .bind(&run.event_id)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("admission settlement reads")
    })
    .await;
    let first_label = event_column(&run, &run.event_id, LABEL).await;
    let settled: bool =
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1")
            .bind(&run.event_id)
            .fetch_one(&mut *run.booted.database.acquire().await.expect("database"))
            .await
            .expect("queue state");
    let second = run.enqueue_event(EventType::Chat).await;
    wait_without_offer(&run, "the next denied event never settled", || async {
        event_column(&run, &second, LABEL).await.as_deref()
            == Some(af_core::event::label::GRANT_DENIED)
    })
    .await;
    let second_label = event_column(&run, &second, LABEL).await;
    supervisor.shutdown().await;
    run.cleanup().await;
    assert_denied_labels(first_label.as_deref(), settled, second_label.as_deref());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_grant_does_not_block_later_events() {
    refusal_drains(false).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn terminal_history_is_settled_without_blocking_later_events() {
    refusal_drains(true).await;
}

async fn connect_credential(run: &Scenario) {
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
            &run.workspace,
            &SecretName::parse(SERVICE).expect("secret name"),
            &SecretBody::parse(&raw).expect("handle body"),
            run.seeded_at,
        )
        .await
        .expect("existing connection");
}

async fn preend(run: &Scenario, preended: bool) {
    if preended {
        // A pre-existing terminal narrative must settle the queue without executing.
        sqlx::query("UPDATE fleet.events SET status = $3, failure_reason = $4 WHERE fleet_id = $1 AND event_id = $2")
            .bind(&run.fleet).bind(&run.event_id)
            .bind(af_core::event::status::GATE_BLOCKED)
            .bind(af_core::event::label::GRANT_DENIED)
            .execute(&mut *run.booted.database.acquire().await.expect("database"))
            .await.expect("terminal narrative");
    }
}

fn assert_denied_labels(first_label: Option<&str>, settled: bool, second_label: Option<&str>) {
    assert_eq!(first_label, Some(af_core::event::label::GRANT_DENIED));
    assert!(settled, "a terminal refusal must settle its admission");
    assert_eq!(
        second_label,
        Some(af_core::event::label::GRANT_DENIED),
        "the next event must be reached after the denied one"
    );
}
