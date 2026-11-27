//! Secret and onboarding HTTP lifecycles over live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use af_core::id::Uuid7;
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{ApiKeySeed, TenantSeed, TestDatabase, UserSeed, WorkspaceSeed, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

const SUBJECT: &str = "user_live_secrets";
const SECRET: &str = "anthropic-live";
/// The person the checkpoint-key case signs in as.
const KEY_SUBJECT: &str = "user_live_checkpoint_key";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn secrets_and_onboarding_round_trip_through_http() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();
    let workspace = format!("/v1/workspaces/{}", fixture.workspace.as_str());

    exercise_secrets(&router, &fixture.token, &workspace).await;

    fixture.cleanup().await;
}

/// The workspace's checkpoint key as the vault stores it: its sealed payload.
async fn stored_checkpoint_key(database: &Db, workspace: &Uuid7) -> Option<Vec<u8>> {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query_scalar(
        "SELECT secret_ciphertext FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2",
    )
    .bind(workspace)
    .bind(afd_vault::CHECKPOINT_KEY_NAME)
    .fetch_optional(&mut *connection)
    .await
    .expect("the vault answers")
}

/// One tenant write to the checkpoint key's name, and what it must answer.
async fn refused_as_reserved(router: &axum::Router, method: Method, uri: &str, token: &str) {
    let body = if method == Method::POST {
        r#"{"name":"checkpoint_key","data":{"key":"tenant-chosen"}}"#
    } else if method == Method::PUT {
        r#"{"data":{"key":"tenant-chosen"}}"#
    } else {
        ""
    };
    let refused = send(router, method.clone(), uri, Some(token), body).await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{method}");
    let problem = json_body(refused).await;
    assert_eq!(
        problem.get("error_code").and_then(Value::as_str),
        Some("UZ-VAULT-006"),
        "{method}"
    );
}

// The tenant sees the row the daemon wrote in its own vault and cannot change
// it: a create before the row exists plants nothing, and a create, replace or
// delete after it leaves the stored bytes as they were.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_checkpoint_secret_is_listed_and_refuses_a_tenant_write() {
    let fixture = Fixture::signed_in_as(KEY_SUBJECT, "checkpoint-key-live@example.test").await;
    fixture.seed().await;
    let fleet = Fleet::live(
        fixture.database.clone(),
        KEY_SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone());
    let keys = afd_api_operator::OperatorPlane::checkpoint_keys(&fleet).clone();
    let router = fleet.router();
    let collection = format!("/v1/workspaces/{}/secrets", fixture.workspace.as_str());
    let item = format!("{collection}/{}", afd_vault::CHECKPOINT_KEY_NAME);

    refused_as_reserved(&router, Method::POST, &collection, &fixture.token).await;
    assert_eq!(
        stored_checkpoint_key(&fixture.database, &fixture.workspace).await,
        None,
        "a tenant cannot plant the key before the daemon writes it"
    );

    keys.save_key(&fixture.workspace)
        .await
        .expect("the daemon writes the workspace's key");
    let written = stored_checkpoint_key(&fixture.database, &fixture.workspace).await;
    assert!(written.is_some());
    let listed =
        json_body(send(&router, Method::GET, &collection, Some(&fixture.token), "").await).await;
    let names: Vec<&str> = listed
        .get("secrets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|secret| secret.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(names, [afd_vault::CHECKPOINT_KEY_NAME]);

    refused_as_reserved(&router, Method::POST, &collection, &fixture.token).await;
    refused_as_reserved(&router, Method::PUT, &item, &fixture.token).await;
    refused_as_reserved(&router, Method::DELETE, &item, &fixture.token).await;
    assert_eq!(
        stored_checkpoint_key(&fixture.database, &fixture.workspace).await,
        written,
        "the stored bytes are unchanged"
    );

    fixture.cleanup().await;
}

async fn exercise_secrets(router: &axum::Router, token: &str, workspace: &str) {
    let collection = format!("{workspace}/secrets");
    create_and_list_secret(router, token, &collection).await;
    replace_secret_and_check_onboarding(router, token, workspace, &collection).await;
    delete_secret_twice(router, token, &collection).await;
}

async fn create_and_list_secret(router: &axum::Router, token: &str, collection: &str) {
    let created = send(
        router,
        Method::POST,
        collection,
        Some(token),
        r#"{"name":"anthropic-live","data":{"provider":"anthropic","api_key":"sk-live"}}"#,
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let duplicate = send(
        router,
        Method::POST,
        collection,
        Some(token),
        r#"{"name":"anthropic-live","data":{"api_key":"sk-other"}}"#,
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let listed = send(router, Method::GET, collection, Some(token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    let held = listed
        .get("secrets")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name") == Some(&Value::from(SECRET)))
        })
        .expect("the stored secret is listed");
    assert_eq!(
        held.get("provider").and_then(Value::as_str),
        Some("anthropic")
    );
    assert!(
        held.get("data").is_none(),
        "plaintext is not a list projection"
    );
}

async fn replace_secret_and_check_onboarding(
    router: &axum::Router,
    token: &str,
    workspace: &str,
    collection: &str,
) {
    let replaced = send(
        router,
        Method::PUT,
        &format!("{collection}/{SECRET}"),
        Some(token),
        r#"{"data":{"provider":"anthropic","api_key":"sk-replaced"}}"#,
    )
    .await;
    assert_eq!(replaced.status(), StatusCode::OK);

    let onboarding = send(
        router,
        Method::GET,
        &format!("{workspace}/onboarding"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(onboarding.status(), StatusCode::OK);
    let onboarding = json_body(onboarding).await;
    assert_eq!(
        onboarding.get("has_secret").and_then(Value::as_bool),
        Some(true)
    );
}

async fn delete_secret_twice(router: &axum::Router, token: &str, collection: &str) {
    for _attempt in 0..2 {
        let deleted = send(
            router,
            Method::DELETE,
            &format!("{collection}/{SECRET}"),
            Some(token),
            "",
        )
        .await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    }
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    workspace: Uuid7,
    user: Uuid7,
    key: Uuid7,
    token: String,
    /// Who the fixture signs in as; unique per test, as the shared lane's
    /// person rows are.
    subject: &'static str,
    email: &'static str,
}

impl Fixture {
    async fn create() -> Self {
        Self::signed_in_as(SUBJECT, "secrets-live@example.test").await
    }

    async fn signed_in_as(subject: &'static str, email: &'static str) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            subject,
            email,
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Secrets live")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("secrets")
            .insert(&mut *connection)
            .await
            .expect("its workspace seeds");
        UserSeed::new(&self.user, &self.tenant, self.subject, self.email)
            .insert(&mut *connection)
            .await
            .expect("its person seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), self.subject)
            .insert(&mut *connection)
            .await
            .expect("its API key seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
