//! Tenant API-key and workspace lifecycles over the migrated schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

const SUBJECT: &str = "user_live_tenant_lifecycle";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_keys_and_workspaces_complete_their_real_lifecycles() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    exercise_api_keys(&router, &fixture.token, &fixture.key).await;
    exercise_workspaces(&router, &fixture.token).await;

    fixture.cleanup().await;
}

async fn exercise_api_keys(router: &axum::Router, token: &str, active_key: &str) {
    assert_key_list(router, token).await;
    let key_id = mint_key(router, token).await;
    assert_duplicate_and_active_delete(router, token, active_key).await;
    revoke_and_delete_key(router, token, &key_id).await;
}

async fn assert_key_list(router: &axum::Router, token: &str) {
    let listed = send(router, Method::GET, "/v1/api-keys", Some(token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        json_body(listed).await.get("total").and_then(Value::as_u64),
        Some(1)
    );
}

async fn mint_key(router: &axum::Router, token: &str) -> String {
    let minted = send(
        router,
        Method::POST,
        "/v1/api-keys",
        Some(token),
        r#"{"key_name":"release","description":"live lifecycle"}"#,
    )
    .await;
    assert_eq!(minted.status(), StatusCode::CREATED);
    assert_eq!(
        minted.headers().get(http::header::CACHE_CONTROL),
        Some(&http::HeaderValue::from_static("no-store, max-age=0"))
    );
    let minted = json_body(minted).await;
    assert!(text(&minted, "key").starts_with("agt_t"));
    text(&minted, "id").to_owned()
}

async fn assert_duplicate_and_active_delete(router: &axum::Router, token: &str, active_key: &str) {
    let duplicate = send(
        router,
        Method::POST,
        "/v1/api-keys",
        Some(token),
        r#"{"key_name":"release"}"#,
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let active_delete = send(
        router,
        Method::DELETE,
        &format!("/v1/api-keys/{active_key}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(active_delete.status(), StatusCode::CONFLICT);
}

async fn revoke_and_delete_key(router: &axum::Router, token: &str, key_id: &str) {
    let revoked = send(
        router,
        Method::PATCH,
        &format!("/v1/api-keys/{key_id}"),
        Some(token),
        r#"{"active":false}"#,
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::OK);
    assert_eq!(
        json_body(revoked)
            .await
            .get("active")
            .and_then(Value::as_bool),
        Some(false)
    );

    let repeated = send(
        router,
        Method::PATCH,
        &format!("/v1/api-keys/{key_id}"),
        Some(token),
        r#"{"active":false}"#,
    )
    .await;
    assert_eq!(repeated.status(), StatusCode::CONFLICT);

    let deleted = send(
        router,
        Method::DELETE,
        &format!("/v1/api-keys/{key_id}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

async fn exercise_workspaces(router: &axum::Router, token: &str) {
    assert_workspace_creation(router, token).await;
    assert_workspace_pages(router, token).await;
}

async fn assert_workspace_creation(router: &axum::Router, token: &str) {
    let first = create_workspace(router, token, r#"{"name":"deploy bots"}"#).await;
    assert_eq!(text(&first, "name"), "deploy bots");
    let generated = create_workspace(router, token, "{}").await;
    assert!(!text(&generated, "name").is_empty());

    let duplicate = send(
        router,
        Method::POST,
        "/v1/workspaces",
        Some(token),
        r#"{"name":"deploy bots"}"#,
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
}

async fn assert_workspace_pages(router: &axum::Router, token: &str) {
    let filtered = send(
        router,
        Method::GET,
        "/v1/tenants/me/workspaces?name=deploy+bots",
        Some(token),
        "",
    )
    .await;
    assert_eq!(filtered.status(), StatusCode::OK);
    assert_eq!(
        json_body(filtered)
            .await
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    let page = send(
        router,
        Method::GET,
        "/v1/tenants/me/workspaces?limit=1",
        Some(token),
        "",
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = json_body(page).await;
    let cursor = text(&page, "next_cursor");
    let next = send(
        router,
        Method::GET,
        &format!("/v1/tenants/me/workspaces?limit=1&starting_after={cursor}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert_eq!(
        json_body(next)
            .await
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
}

async fn create_workspace(router: &axum::Router, token: &str, body: &str) -> Value {
    let response = send(router, Method::POST, "/v1/workspaces", Some(token), body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await
}

fn text<'value>(document: &'value Value, field: &str) -> &'value str {
    document
        .get(field)
        .expect("the response field exists")
        .as_str()
        .expect("the response field is a string")
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    key: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let first = mint_id().replace('-', "");
        let second = mint_id().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            key: mint_id(),
            token: format!("agt_t{first}{second}"),
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Tenant lifecycle', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($2::uuid, $1::uuid, 'fixture', '', $3, $4, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(SUBJECT)
        .execute(&mut *connection)
        .await
        .expect("the tenant credential seeds");
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
