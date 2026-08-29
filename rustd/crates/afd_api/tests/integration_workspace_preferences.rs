//! Preference, onboarding, and secret HTTP lifecycles over live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

const SUBJECT: &str = "user_live_preferences_and_secrets";
const SECRET: &str = "anthropic-live";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn preferences_onboarding_and_secrets_round_trip_through_http() {
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

    exercise_preferences(&router, &fixture.token, &workspace).await;
    exercise_secrets(&router, &fixture.token, &workspace).await;

    fixture.cleanup().await;
}

async fn exercise_preferences(router: &axum::Router, token: &str, workspace: &str) {
    let empty = send(
        router,
        Method::GET,
        &format!("{workspace}/preferences"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(
        json_body(empty).await.get("prefs"),
        Some(&serde_json::json!({}))
    );

    let written = send(
        router,
        Method::PUT,
        &format!("{workspace}/preferences/getting_started_dismissed"),
        Some(token),
        "true",
    )
    .await;
    assert_eq!(written.status(), StatusCode::OK);
    assert_eq!(
        json_body(written)
            .await
            .pointer("/prefs/getting_started_dismissed")
            .and_then(Value::as_bool),
        Some(true)
    );
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
        onboarding.get("dismissed").and_then(Value::as_bool),
        Some(true)
    );
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
    tenant: String,
    workspace: Uuid7,
    user: String,
    key: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Preferences live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'preferences', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'preferences-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(SUBJECT)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the preference principal and workspace seed");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
