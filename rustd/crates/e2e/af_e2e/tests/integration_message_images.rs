//! The tenant message route stores a picture with the same turn it acknowledges.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture and JSON assertions"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use afd_db::test_util::{ModelSeed, mint_id};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{PROVIDER, Scenario, scenario_with_provider};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::{mint_tenant_token, provider_granting};

const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAb0lEQVR4nO3PAQkAAAyEwO9feoshgnABdLep8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3IPanc8OLDQitxAAAAAElFTkSuQmCC";
const CHAT_MODEL: &str = "image-test-text-model";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_steer_keeps_the_image_and_replays_the_same_turn() {
    let mut supervisor = Supervisor::new();
    let provider = provider_granting("fleet:read fleet:write").await;
    let scene = scenario_with_provider(&mut supervisor, Some(&provider)).await;
    let fleet = &scene.fleet;
    let token = mint_tenant_token();
    seed_tenant_key(&scene.booted, &scene.tenant, &token, scene.seeded_at).await;
    let body = json!({"message": "What colour is this?", "operation_id": "picture",
        "images": [{"media_type": "image/png", "data": PICTURE}]});
    let client = afd_identity::direct(Duration::from_secs(10))
        .build()
        .expect("tenant client");
    let path = format!(
        "{}/v1/workspaces/{}/fleets/{fleet}/messages",
        scene.base, scene.workspace
    );

    let first = post(&client, &path, &token, &body, 202).await;
    let retry = post(&client, &path, &token, &body, 202).await;
    assert_eq!(first["turn_id"], retry["turn_id"]);
    assert_eq!(retry["replayed"], true);
    let turns = image_turns(&scene).await;
    assert_eq!(turns.len(), 1);
    let stored: Value = serde_json::from_str(&turns[0]).expect("the stored message");
    assert_eq!(stored["images"], body["images"]);
    assert_eq!(stored["message"], body["message"]);
    // A new request on a newly selected text model is refused. An operation
    // admitted earlier still answers with the same receipt after the switch.
    let catalogue = select_text_model(&scene, fleet).await;
    let mut next = body.clone();
    next["operation_id"] = json!("next-picture");
    let refused = post(&client, &path, &token, &next, 409).await;
    assert_eq!(refused["error_code"], "UZ-AGT-027");
    assert_eq!(
        refused["detail"],
        format!("Model {CHAT_MODEL} cannot receive images")
    );
    assert_eq!(image_turns(&scene).await.len(), 1);
    assert_eq!(
        post(&client, &path, &token, &body, 202).await["turn_id"],
        first["turn_id"]
    );
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("catalogue connection");
    sqlx::query("DELETE FROM model.platform_models WHERE id = $1")
        .bind(&catalogue)
        .execute(&mut *connection)
        .await
        .expect("remove test catalogue entry");
    drop(connection);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

async fn image_turns(scene: &Scenario) -> Vec<String> {
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("ledger connection");
    sqlx::query_scalar("SELECT request_json::text FROM thread.turns WHERE thread_id = $1 AND request_json ? 'images'")
        .bind(&scene.fleet).fetch_all(&mut *connection).await.expect("image turns")
}

async fn select_text_model(scene: &Scenario, fleet: &Uuid7) -> Uuid7 {
    let catalogue = mint_id();
    let entry = mint_id();
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("catalogue connection");
    ModelSeed::new(&catalogue, PROVIDER, CHAT_MODEL, 200_000)
        .rates(3_000_000_000, 300_000_000, 15_000_000_000)
        .verified_at(scene.seeded_at)
        .inference(
            r#"{"api":"chat","base_url":"https://api.anthropic.com/v1","max_output_tokens":8192}"#,
        )
        .insert(&mut *connection)
        .await
        .expect("text model catalogue");
    sqlx::query(
        "INSERT INTO model.tenant_models
        (id, tenant_id, model_name, secret_ref, workspace_id, selected, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, false, now(), now())",
    )
    .bind(&entry)
    .bind(&scene.tenant)
    .bind(CHAT_MODEL)
    .bind(PROVIDER)
    .bind(&scene.workspace)
    .execute(&mut *connection)
    .await
    .expect("text model entry");
    crate::e2e_native::select_model(&mut connection, fleet, entry.as_str()).await;
    catalogue
}

async fn post(
    client: &reqwest::Client,
    path: &str,
    token: &str,
    body: &Value,
    status: u16,
) -> Value {
    let response = client
        .post(path)
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .expect("HTTP answer");
    let received = response.status().as_u16();
    let result: Value = response.json().await.expect("JSON answer");
    assert_eq!(received, status, "{result}");
    result
}
