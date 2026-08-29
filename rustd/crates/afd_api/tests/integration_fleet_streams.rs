//! Workspace stream opening over live Postgres and Redis.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use std::time::Duration;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::SubscriptionHub;
use futures_util::StreamExt as _;
use http::{Method, StatusCode};

use self::harness::{Fleet, send};

const SUBJECT: &str = "user_live_workspace_stream";

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_workspace_stream_announces_its_live_fleet_set() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let hub = SubscriptionHub::start(harness::redis_config())
        .await
        .expect("the lane's subscription connection starts");
    let fleet = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_live_hub(hub.clone());
    let ownership = fleet.ownership();
    let fleet_store = fleet.fleet_store();
    let router = fleet.router();
    let mut body = open_stream(&router, &fixture).await;

    let second = fixture.seed_second_fleet().await;
    fleet_store.invalidate_live_set(&fixture.workspace).await;
    let refreshed = fleet_store
        .live_set(&fixture.workspace)
        .await
        .expect("the invalidated set refreshes before the clock is paused");
    assert!(refreshed.contains(&second));
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    let changed = next_chunk(&mut body).await;
    assert!(changed.contains("event: hello"));
    assert!(changed.contains(&second));

    ownership.revoke();
    tokio::time::advance(Duration::from_secs(11)).await;
    assert!(
        stream_ends(&mut body).await,
        "revoked membership closes the wall"
    );
    drop(body);
    hub.shutdown();
    tokio::time::resume();
    fixture.cleanup().await;
}

async fn open_stream(router: &axum::Router, fixture: &Fixture) -> axum::body::BodyDataStream {
    let response = send(
        router,
        Method::GET,
        &format!(
            "/v1/workspaces/{}/events/stream",
            fixture.workspace.as_str()
        ),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    let mut body = response.into_body().into_data_stream();
    let chunk = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("the opening announcement is immediate")
        .expect("the stream stays open")
        .expect("the SSE body is infallible");
    let opening = std::str::from_utf8(&chunk).expect("SSE is UTF-8");
    assert!(opening.contains("event: hello"));
    assert!(opening.contains(&fixture.fleet));
    body
}

async fn next_chunk(body: &mut axum::body::BodyDataStream) -> String {
    let chunk = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("the expected wall transition is prompt")
        .expect("the stream stays open for the transition")
        .expect("the SSE body is infallible");
    std::str::from_utf8(&chunk)
        .expect("SSE is UTF-8")
        .to_owned()
}

async fn stream_ends(body: &mut axum::body::BodyDataStream) -> bool {
    for _frame in 0..3 {
        match body.next().await {
            None => return true,
            Some(Ok(_heartbeat)) => {}
            Some(Err(_infallible)) => return false,
        }
    }
    false
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    fleet: String,
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
            fleet: mint_id(),
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
               VALUES ($1::uuid, 'Workspace stream', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'stream', $3, 1) \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, 'fixture', '', $5, $3, TRUE, NULL, 1, 1) \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($6::uuid, $2::uuid, $1::uuid, 'streamed', '# fixture', '{}', \
                     'active', 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(SUBJECT)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(&self.fleet)
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace and fleet seed");
    }

    async fn seed_second_fleet(&self) -> String {
        let fleet = mint_id();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'streamed-second', '# fixture', '{}', \
                     'active', 2, 2)",
        )
        .bind(&fleet)
        .bind(self.workspace.as_str())
        .bind(&self.tenant)
        .execute(&mut *connection)
        .await
        .expect("the second live fleet seeds");
        fleet
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
