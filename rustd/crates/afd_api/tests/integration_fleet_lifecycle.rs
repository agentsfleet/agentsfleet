//! Fleet lifecycle and event history HTTP paths over the live stores.
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
use http::{Method, StatusCode, header};
use serde_json::Value;

use self::harness::{Fleet, json_body, send, send_with_headers};
#[path = "fleet_lifecycle_live/message.rs"]
mod message;

const SUBJECT: &str = "user_live_fleet_lifecycle";
const EVENT: &str = "1760000000000-0";
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn fleet_and_event_http_lifecycles_use_the_live_stores() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_fleet_queue(fixture.database.clone(), queue.clone())
    .with_steering_queue(queue)
    .router();
    let workspace = format!("/v1/workspaces/{}", fixture.workspace.as_str());

    let fleet = install(&router, &fixture, &workspace).await;
    fixture.seed_event_and_grant(&fleet).await;
    message::exercise(&router, &fixture, &workspace, &fleet).await;
    exercise_history(&router, &fixture.token, &workspace, &fleet).await;
    exercise_grants(&router, &fixture, &workspace, &fleet).await;
    exercise_edit_and_purge(&router, &fixture.token, &workspace, &fleet).await;

    fixture.cleanup().await;
}
async fn exercise_grants(router: &axum::Router, fixture: &Fixture, workspace: &str, fleet: &Uuid7) {
    let collection = format!("{workspace}/fleets/{}/integration-grants", fleet.as_str());
    let listed = send(router, Method::GET, &collection, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        json_body(listed)
            .await
            .pointer("/items/0/id")
            .and_then(Value::as_str),
        Some(fixture.grant.as_str())
    );
    let revoked = send(
        router,
        Method::DELETE,
        &format!("{collection}/{}", fixture.grant),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let repeated = send(
        router,
        Method::DELETE,
        &format!("{collection}/{}", fixture.grant),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(repeated.status(), StatusCode::NOT_FOUND);
}
async fn install(router: &axum::Router, fixture: &Fixture, workspace: &str) -> Uuid7 {
    let installed = send(
        router,
        Method::POST,
        &format!("{workspace}/fleets"),
        Some(&fixture.token),
        &serde_json::json!({
            "platform_library_id": fixture.library,
            "name": "live-fleet"
        })
        .to_string(),
    )
    .await;
    let status = installed.status();
    let installed = json_body(installed).await;
    assert_eq!(status, StatusCode::CREATED, "{installed}");
    assert_eq!(
        installed.get("status").and_then(Value::as_str),
        Some("active")
    );
    Uuid7::parse(
        installed
            .get("fleet_id")
            .and_then(Value::as_str)
            .expect("an install returns its fleet"),
    )
    .expect("the installed fleet id is canonical")
}

async fn exercise_history(router: &axum::Router, token: &str, workspace: &str, fleet: &Uuid7) {
    let collection = format!("{workspace}/events?fleet_id={}", fleet.as_str());
    let workspace_page = send(router, Method::GET, &collection, Some(token), "").await;
    assert_eq!(workspace_page.status(), StatusCode::OK);
    assert_eq!(
        json_body(workspace_page)
            .await
            .pointer("/items/0/event_id")
            .and_then(Value::as_str),
        Some(EVENT)
    );

    let fleet_events = format!("{workspace}/fleets/{}/events", fleet.as_str());
    let page = send(
        router,
        Method::GET,
        &format!("{fleet_events}?actor_prefix=steer"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        json_body(page)
            .await
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    let detail = send(
        router,
        Method::GET,
        &format!("{fleet_events}/{EVENT}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = json_body(detail).await;
    let request = detail
        .get("request_json")
        .and_then(Value::as_str)
        .expect("event detail carries its opaque request JSON");
    assert_eq!(
        serde_json::from_str::<Value>(request).expect("stored request JSON remains valid"),
        serde_json::json!({"prompt":"ship"})
    );
}

async fn exercise_edit_and_purge(
    router: &axum::Router,
    token: &str,
    workspace: &str,
    fleet: &Uuid7,
) {
    const CHANGED_SKILL: &str =
        "---\nname: live-fleet\ndescription: Changed fleet.\nversion: 1.0.1\n---\nRun changed.\n";
    const STALE_SKILL: &str =
        "---\nname: live-fleet\ndescription: Stale fleet.\nversion: 1.0.2\n---\nRun stale.\n";
    let item = format!("{workspace}/fleets/{}", fleet.as_str());
    let listed = send(
        router,
        Method::GET,
        &format!("{workspace}/fleets"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let detail = send(router, Method::GET, &item, Some(token), "").await;
    assert_eq!(detail.status(), StatusCode::OK);
    let etag = detail
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("fleet detail carries an etag")
        .to_owned();

    let edited = send_with_headers(
        router,
        Method::PATCH,
        &item,
        Some(token),
        &serde_json::json!({"source_markdown": CHANGED_SKILL}).to_string(),
        &[(header::IF_MATCH, &etag)],
    )
    .await;
    let status = edited.status();
    let edited = json_body(edited).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let stale = send_with_headers(
        router,
        Method::PATCH,
        &item,
        Some(token),
        &serde_json::json!({"source_markdown": STALE_SKILL}).to_string(),
        &[(header::IF_MATCH, &etag)],
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

    for status in ["stopped", "killed"] {
        let changed = send(
            router,
            Method::PATCH,
            &item,
            Some(token),
            &serde_json::json!({"status": status}).to_string(),
        )
        .await;
        assert_eq!(changed.status(), StatusCode::OK, "transition to {status}");
    }
    let purged = send(router, Method::DELETE, &item, Some(token), "").await;
    assert_eq!(purged.status(), StatusCode::NO_CONTENT);
    let missing = send(router, Method::GET, &item, Some(token), "").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    user: String,
    key: String,
    library: String,
    grant: String,
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
            library: mint_id(),
            grant: mint_id(),
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
               VALUES ($1::uuid, 'Fleet lifecycle', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'fleets', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'fleet-live@example.test', 1, 1) \
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
        .expect("the authenticated workspace seeds");
        sqlx::query(
            "INSERT INTO core.fleet_library \
               (id, name, description, source_repo, source_path, source_ref, \
                required_credentials, required_credentials_reasons, required_tools, \
                network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
                created_at, updated_at) \
             VALUES ($1, 'live-fleet', 'fixture', 'repo', 'path', 'main', '[]', '{}', \
                     '[]', '[]', 'public', $2, $3, $4, 1, 1)",
        )
        .bind(&self.library)
        .bind(format!("sha256:{}", self.library))
        .bind("---\nname: live-fleet\ndescription: Live fleet.\nversion: 1.0.0\n---\nRun.\n")
        .bind("---\nname: live-fleet\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n")
        .execute(&mut *connection)
        .await
        .expect("the platform library entry seeds");
    }

    async fn seed_event_and_grant(&self, fleet: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, \
                response_text, tokens, wall_ms, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', 'completed', \
                     '{\"prompt\":\"ship\"}', 'done', 7, 12, 10, 11)",
        )
        .bind(fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(EVENT)
        .execute(&mut *connection)
        .await
        .expect("the completed event seeds");
        sqlx::query(
            "INSERT INTO core.integration_grants \
               (id, fleet_id, service, status, requested_reason, approved_at, created_at) \
             VALUES ($1::uuid, $2::uuid, 'github', 'approved', 'fixture', 10, 9)",
        )
        .bind(&self.grant)
        .bind(fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the approved grant seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        sqlx::query("DELETE FROM core.fleet_library WHERE id = $1")
            .bind(&self.library)
            .execute(&mut *connection)
            .await
            .expect("the platform library fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
