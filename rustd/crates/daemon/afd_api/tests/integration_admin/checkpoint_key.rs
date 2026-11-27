//! A platform operator rotates one workspace's checkpoint key over live Postgres.

use super::{Fixture, Fleet, SUBJECT, json_body, send};
use af_core::id::Uuid7;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::mint_id;
use futures_util::FutureExt as _;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use std::panic::AssertUnwindSafe;

fn rotate_path(workspace: &Uuid7) -> String {
    format!("/v1/admin/workspaces/{workspace}/checkpoint-key/rotate")
}

/// One stored object of the workspace, as a registration leaves it.
async fn seed_object(fixture: &Fixture) -> Uuid7 {
    let object = mint_id();
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO runner.checkpoint_objects (id, workspace_id, repository, content_identity,
            plaintext_digest, plaintext_size, kind, format_version, bucket_reference, cipher,
            accounted_bytes, owner_checkpoint_id, owner_run_id, owner_fence, object_nonce,
            reclaim_after_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 8, $6, 1, $7, $8, 24, $1, $1, 1, $9, now(), now(), now())",
    )
    .bind(&object)
    .bind(&fixture.workspace)
    .bind("turn:rotation-fixture")
    .bind("c".repeat(64))
    .bind("d".repeat(64))
    .bind("file")
    .bind("fixture-bucket")
    .bind(<&str>::from(
        af_wire::durable_task::CheckpointCipher::Aes256Gcm,
    ))
    .bind([7_u8; af_crypto::NONCE_LEN].as_slice())
    .execute(&mut *connection)
    .await
    .expect("a stored object seeds");
    object
}

/// The workspace's object rows, and the bucket deletes it is owed.
async fn custody(fixture: &Fixture) -> (i64, i64) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM runner.checkpoint_objects WHERE workspace_id = $1),
                (SELECT count(*) FROM runner.checkpoint_purges
                  WHERE workspace_id = $1 AND completed_at IS NULL)",
    )
    .bind(&fixture.workspace)
    .fetch_one(&mut *connection)
    .await
    .expect("the checkpoint tables answer")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_operator_rotates_a_workspaces_checkpoint_key() {
    let fixture = Fixture::create().await;
    let result = AssertUnwindSafe(exercise(&fixture)).catch_unwind().await;
    fixture.cleanup().await;
    if let Err(failure) = result {
        std::panic::resume_unwind(failure);
    }
}

async fn exercise(fixture: &Fixture) {
    fixture.seed().await;
    let operator = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    );
    let keys = afd_api_operator::OperatorPlane::checkpoint_keys(&operator).clone();
    let operator = operator.with_live_ownership().router();
    let path = rotate_path(&fixture.workspace);
    let token = Some(fixture.token.as_str());

    let before = keys
        .save_key(&fixture.workspace)
        .await
        .expect("a first save writes the key");
    seed_object(fixture).await;
    assert_eq!(custody(fixture).await, (1, 0));

    let rotated = send(&operator, Method::POST, &path, token, "").await;
    let status = rotated.status();
    let rotated = json_body(rotated).await;
    assert_eq!(status, StatusCode::OK, "{rotated}");
    assert_eq!(
        rotated,
        json!({"retired_generations": 0, "retired_objects": 1})
    );
    assert_eq!(custody(fixture).await, (0, 1), "its bucket delete is owed");
    let after = keys
        .save_key(&fixture.workspace)
        .await
        .expect("the new key loads");
    assert_ne!(after.expose(), before.expose(), "the key's bytes changed");

    let again = send(&operator, Method::POST, &path, token, "").await;
    assert_eq!(
        json_body(again).await,
        json!({"retired_generations": 0, "retired_objects": 0})
    );
    let held = keys
        .save_key(&fixture.workspace)
        .await
        .expect("the second rotation's key loads");
    assert_ne!(held.expose(), after.expose(), "a repeat rotates again");
    refusals(fixture, &operator, &keys, &held).await;
}

/// A tenant principal, a malformed identifier and a workspace nobody holds are
/// each refused, and none of them changes the key.
async fn refusals(
    fixture: &Fixture,
    operator: &axum::Router,
    keys: &afd_fleet::durable_task::storage::CheckpointKeys,
    held: &af_crypto::secret::SecretBytes,
) {
    let path = rotate_path(&fixture.workspace);
    let token = Some(fixture.token.as_str());
    let tenant = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::SecretWrite, Scope::FleetAdmin, Scope::WorkspaceAdmin]),
    )
    .with_live_ownership()
    .router();
    let refused = send(&tenant, Method::POST, &path, token, "").await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(refused)
            .await
            .get("error_code")
            .and_then(Value::as_str),
        Some("UZ-AUTH-022")
    );

    let malformed = "/v1/admin/workspaces/not-a-workspace/checkpoint-key/rotate";
    let malformed = send(operator, Method::POST, malformed, token, "").await;
    assert!(malformed.status().is_client_error());

    let unknown = send(operator, Method::POST, &rotate_path(&mint_id()), token, "").await;
    let status = unknown.status();
    let unknown = json_body(unknown).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{unknown}");
    assert_eq!(
        unknown.get("error_code").and_then(Value::as_str),
        Some("UZ-AUTH-001")
    );

    let unchanged = keys
        .save_key(&fixture.workspace)
        .await
        .expect("the key still loads");
    assert_eq!(unchanged.expose(), held.expose());
}
