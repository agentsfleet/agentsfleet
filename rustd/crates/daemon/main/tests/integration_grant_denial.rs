//! A connected workspace still needs fleet approval; denial must drain the queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::e2e::{GOOD_KEK, Scenario, scenario};
use crate::reads::event_column;
use crate::wire::capable_beat;
use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::turn::TurnKind;
use afd_approval::{Decision, Inbox};
use afd_fleet::run::Runs;
use afd_vault::{SecretBody, SecretName, Vault};
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::sync::Arc;

const SERVICE: &str = "github";
const LABEL: &str = "COALESCE(failure_reason, '')";

async fn connect_and_declare(scene: &Scenario) {
    connect_credential(scene).await;
    // Declared the way a person declares it: an Apply selecting a setup that
    // names the credential. The run reads the selected revision, never a
    // copy on the fleet row.
    let mut connection = scene.booted.database.acquire().await.expect("connection");
    let mut config: serde_json::Value = sqlx::query_scalar::<_, String>(
        "SELECT (r.definition -> 'config')::text FROM fleet.fleets f \
         JOIN fleet.revisions r ON r.id = f.head_revision_id WHERE f.id = $1",
    )
    .bind(&scene.fleet)
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
    crate::e2e_native::reselect(&mut connection, &scene.fleet, &definition).await;
    drop(connection);
    // Prefer this fixture's fleet over unrelated ready marks left by other suites.
    Runs::new(
        scene.booted.database.clone(),
        scene.booted.bus.clone(),
        Entropy::new(),
    )
    .claim(&scene.fleet, &scene.runner_id, std::time::Duration::ZERO)
    .await
    .expect("initial affinity claim")
    .expect("new fleet is unclaimed");
}

/// How many approval cards this fixture's fleet has raised.
async fn gate_cards(scene: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1")
        .bind(&scene.fleet)
        .fetch_one(&mut *scene.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the gate count must scene")
}

/// Wait for the durable outcome while refusing any assignment for this fleet.
async fn wait_without_assignment<F, Fut>(scene: &Scenario, awaited: &str, mut reached: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let observed = crate::wire::wait_without_assignment(scene, || {
        let reached = reached();
        async {
            sqlx::query(
            "UPDATE thread.threads SET run_expires_at = clock_timestamp() - interval '1 millisecond' \
             WHERE fleet_id = $1",
        )
        .bind(&scene.fleet)
        .execute(&mut *scene.booted.database.acquire().await.expect("connection"))
        .await
        .expect("expire the previous no-work claim");
            afd_bus::wake::publish(&scene.booted.bus, scene.fleet.clone()).await;
            reached.await
        }
    })
    .await;
    assert!(observed, "{awaited}; inspect AFD_TEST_LOG=1");
}

async fn deny(scene: &Scenario) {
    let actions: Vec<Uuid7> =
        sqlx::query_scalar("SELECT action_id FROM fleet.approval_gates WHERE fleet_id = $1")
            .bind(&scene.fleet)
            .fetch_all(&mut *scene.booted.database.acquire().await.expect("connection"))
            .await
            .expect("actionable cards");
    assert_eq!(
        actions.len(),
        1,
        "a connected workspace gets one fleet approval card"
    );
    Inbox::new(
        scene.booted.database.clone(),
        scene.booted.bus.clone(),
        afd_turns::Turns::for_tests(scene.booted.database.clone(), scene.booted.bus.clone()),
    )
    .resolve(
        actions.first().expect("card").as_str(),
        Decision::Denied,
        "fixture",
        "",
        Some(scene.fleet.as_str()),
    )
    .await
    .expect("denial");
}

async fn refusal_drains(preended: bool) {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    connect_and_declare(&scene).await;
    let beat = crate::wire::heartbeat(&scene, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );
    wait_without_assignment(&scene, "the gate never saw this fleet", async || {
        gate_cards(&scene).await == 1
    })
    .await;
    assert_eq!(
        gate_cards(&scene).await,
        1,
        "the session claim raised its approval card"
    );
    deny(&scene).await;
    preend(&scene, preended).await;
    wait_without_assignment(&scene, "the denied admission never settled", async || {
        sqlx::query_scalar::<_, bool>(
            "SELECT settled_at IS NOT NULL FROM thread.turns WHERE id = $1",
        )
        .bind(&scene.turn_id)
        .fetch_one(&mut *scene.booted.database.acquire().await.expect("connection"))
        .await
        .expect("admission settlement reads")
    })
    .await;
    let first_label = event_column(&scene, &scene.turn_id, LABEL).await;
    let settled: bool =
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM thread.turns WHERE id = $1")
            .bind(&scene.turn_id)
            .fetch_one(&mut *scene.booted.database.acquire().await.expect("database"))
            .await
            .expect("queue state");
    let second = scene.enqueue_event(TurnKind::Chat).await;
    wait_without_assignment(&scene, "the next denied event never settled", async || {
        event_column(&scene, &second, LABEL).await.as_deref()
            == Some(af_core::turn::label::GRANT_DENIED)
    })
    .await;
    let second_label = event_column(&scene, &second, LABEL).await;
    supervisor.shutdown().await;
    scene.cleanup().await;
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

async fn connect_credential(scene: &Scenario) {
    let vault = Vault::new(
        scene.booted.database.clone(),
        Arc::new(Kek::from_hex(GOOD_KEK).expect("fixture key")),
        Entropy::new(),
    );
    let raw = serde_json::value::RawValue::from_string(
        json!({"integration": SERVICE, "app_id": "7", "installation_id": "42"}).to_string(),
    )
    .expect("handle JSON");
    vault
        .create(
            &scene.workspace,
            &SecretName::parse(SERVICE).expect("secret name"),
            &SecretBody::parse(&raw).expect("handle body"),
            scene.seeded_at,
        )
        .await
        .expect("existing connection");
}

async fn preend(scene: &Scenario, preended: bool) {
    if preended {
        // A pre-existing terminal narrative must settle the queue without executing.
        sqlx::query("UPDATE thread.turns SET status = $3, failure_reason = $4 WHERE thread_id = $1 AND id = $2")
            .bind(&scene.fleet).bind(&scene.turn_id)
            .bind(af_core::turn::status::GATE_BLOCKED)
            .bind(af_core::turn::label::GRANT_DENIED)
            .execute(&mut *scene.booted.database.acquire().await.expect("database"))
            .await.expect("terminal narrative");
    }
}

fn assert_denied_labels(first_label: Option<&str>, settled: bool, second_label: Option<&str>) {
    assert_eq!(first_label, Some(af_core::turn::label::GRANT_DENIED));
    assert!(settled, "a terminal refusal must settle its admission");
    assert_eq!(
        second_label,
        Some(af_core::turn::label::GRANT_DENIED),
        "the next event must be reached after the denied one"
    );
}
