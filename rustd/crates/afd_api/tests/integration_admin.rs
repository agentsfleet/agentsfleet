//! Platform catalogue, import, and default-key lifecycles over live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]
use self::harness::{Fleet, json_body, send};
use crate::harness;
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;
#[path = "admin_live/libraries.rs"]
mod libraries;
const SUBJECT: &str = "user_live_platform_catalogue";
const MODEL_CREATE: &str = r#"{"provider":"fixture","model_id":"fixture-model","context_cap_tokens":200000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}"#;
const MODEL_UPDATE: &str = r#"{"context_cap_tokens":250000,"input_nanos_per_mtok":6,"cached_input_nanos_per_mtok":2,"output_nanos_per_mtok":30}"#;
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_models_and_libraries_complete_real_http_lifecycles() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();
    exercise_models(&router, &fixture.token).await;
    libraries::exercise(&router, &fixture.token, &fixture.slug).await;
    exercise_platform_keys(&router, &fixture).await;
    fixture.cleanup().await;
}

async fn exercise_platform_keys(router: &axum::Router, fixture: &Fixture) {
    let collection = "/v1/admin/platform-keys";
    let unknown = send(
        router,
        Method::PUT,
        collection,
        Some(&fixture.token),
        &serde_json::json!({
            "provider": fixture.provider,
            "source_workspace_id": mint_id(),
            "model": fixture.platform_model,
        })
        .to_string(),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    // The status alone never distinguished this from a malformed body. An id
    // that parses but names no workspace is not the caller mistyping something,
    // and telling them to re-check their input sends them looking at a value
    // that was already correct.
    assert_eq!(
        json_body(unknown).await.get("code").and_then(Value::as_str),
        Some("UZ-PROVIDER-011"),
        "an absent workspace must not read back as an invalid request"
    );

    let set = send(
        router,
        Method::PUT,
        collection,
        Some(&fixture.token),
        &fixture.platform_key_body(),
    )
    .await;
    assert_eq!(set.status(), StatusCode::OK);
    assert_eq!(
        json_body(set).await.get("active").and_then(Value::as_bool),
        Some(true)
    );
    let listed = send(router, Method::GET, collection, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert!(contains_provider(
        &json_body(listed).await,
        &fixture.provider
    ));

    for _attempt in 0..2 {
        let deactivated = send(
            router,
            Method::DELETE,
            &format!("{collection}/{}", fixture.provider),
            Some(&fixture.token),
            "",
        )
        .await;
        assert_eq!(deactivated.status(), StatusCode::OK);
        assert_eq!(
            json_body(deactivated)
                .await
                .get("active")
                .and_then(Value::as_bool),
            Some(false)
        );
    }
}

async fn exercise_models(router: &axum::Router, token: &str) {
    let created = send(
        router,
        Method::POST,
        "/v1/admin/models",
        Some(token),
        MODEL_CREATE,
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_body(created).await;
    let id = created
        .get("id")
        .and_then(Value::as_str)
        .expect("the created model returns its id")
        .to_owned();

    let duplicate = send(
        router,
        Method::POST,
        "/v1/admin/models",
        Some(token),
        MODEL_CREATE,
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let listed = send(router, Method::GET, "/v1/admin/models", Some(token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert!(contains_id(&json_body(listed).await, "models", &id));

    let updated = send(
        router,
        Method::PATCH,
        &format!("/v1/admin/models/{id}"),
        Some(token),
        MODEL_UPDATE,
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);

    let deleted = send(
        router,
        Method::DELETE,
        &format!("/v1/admin/models/{id}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let missing = send(
        router,
        Method::DELETE,
        &format!("/v1/admin/models/{id}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

fn contains_id(document: &Value, collection: &str, expected: &str) -> bool {
    document
        .get(collection)
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("id").and_then(Value::as_str) == Some(expected))
        })
}

fn contains_provider(document: &Value, expected: &str) -> bool {
    document
        .get("keys")
        .and_then(Value::as_array)
        .is_some_and(|keys| {
            keys.iter()
                .any(|key| key.get("provider").and_then(Value::as_str) == Some(expected))
        })
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    key: String,
    token: String,
    slug: String,
    workspace: String,
    platform_model_row: String,
    platform_model: String,
    provider: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let first = mint_id().replace('-', "");
        let second = mint_id().replace('-', "");
        let suffix = mint_id().replace('-', "");
        let provider = format!("live{}", &suffix[..8]);
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            key: mint_id(),
            token: format!("agt_t{first}{second}"),
            slug: format!("live-{suffix}"),
            workspace: mint_id(),
            platform_model_row: mint_id(),
            platform_model: "platform-key-model".to_owned(),
            provider,
            lane,
        }
    }

    fn platform_key_body(&self) -> String {
        serde_json::json!({
            "provider": self.provider,
            "source_workspace_id": self.workspace,
            "model": self.platform_model,
        })
        .to_string()
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Platform catalogue', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($5::uuid, $1::uuid, 'platform-default', $4, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($2::uuid, $1::uuid, 'platform', '', $3, $4, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(SUBJECT)
        .bind(&self.workspace)
        .execute(&mut *connection)
        .await
        .expect("the platform credential seeds");
        sqlx::query(
            "INSERT INTO core.model_library \
               (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
                cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, 200000, 5, 1, 25, 1, 1)",
        )
        .bind(&self.platform_model_row)
        .bind(&self.platform_model)
        .bind(&self.provider)
        .execute(&mut *connection)
        .await
        .expect("the platform-default model seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.platform_provider_defaults WHERE provider = $1")
            .bind(&self.provider)
            .execute(&mut *connection)
            .await
            .expect("the platform default cleanup runs");
        sqlx::query("DELETE FROM core.model_library WHERE id = $1::uuid")
            .bind(&self.platform_model_row)
            .execute(&mut *connection)
            .await
            .expect("the platform model cleanup runs");
        sqlx::query("DELETE FROM core.fleet_library WHERE id = $1")
            .bind(&self.slug)
            .execute(&mut *connection)
            .await
            .expect("the scoped library cleanup runs");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped credential cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
