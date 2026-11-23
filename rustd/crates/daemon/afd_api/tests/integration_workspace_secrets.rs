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
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
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
        UserSeed::new(
            &self.user,
            &self.tenant,
            SUBJECT,
            "secrets-live@example.test",
        )
        .insert(&mut *connection)
        .await
        .expect("its person seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), SUBJECT)
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
