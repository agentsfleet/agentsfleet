//! The workspace stream's live fixture: a seeded tenant, workspace and fleet,
//! the stream opened on them, and SSE events read whole.
//!
//! Split from `integration_fleet_streams.rs` by concern, so the wall's tick
//! tests share it rather than copying it.

use std::time::Duration;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use futures_util::StreamExt as _;
use http::{Method, StatusCode};

use afd_auth::scope::{Scope, ScopeSet};
use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::streams::{FleetStreams, fleet_activity_channel};
use afd_fleet_lifecycle::Fleets;
use axum::body::BodyDataStream;

use crate::harness::{self, Fleet, OneWorkspace, send};

/// How long a quiet stream is watched to show it says nothing.
const QUIET: Duration = Duration::from_millis(1_500);

/// A frame of activity, as the runner's chunk publishes it.
const CHUNK: &str = r#"{"kind":"chunk","event_id":"e","text":"live"}"#;

/// The subject the fixture's workspace and key are created by.
pub(crate) const SUBJECT: &str = "user_live_workspace_stream";

pub(crate) async fn open_stream(
    router: &axum::Router,
    fixture: &Fixture,
) -> axum::body::BodyDataStream {
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
    announces_counters(opening, fixture).await;
    body
}

/// The opening `hello` says where each fleet stands, read fresh for it: a
/// fleet that has never run answers with zeros rather than being left out.
async fn announces_counters(opening: &str, fixture: &Fixture) {
    let data = opening
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .expect("the hello carries a data line");
    let hello: serde_json::Value = serde_json::from_str(data.trim()).expect("the hello is JSON");
    let counters = afd_events::fleet_counters(&fixture.database, &fixture.fleet)
        .await
        .expect("the counters read back");
    assert_eq!(
        hello.pointer(&format!("/counters/{}/events_processed", fixture.fleet)),
        Some(&serde_json::json!(counters.events_processed)),
        "the hello carries the fleet's event count: {hello}"
    );
    assert_eq!(
        hello.pointer(&format!("/counters/{}/budget_used_nanos", fixture.fleet)),
        Some(&serde_json::json!(counters.budget_used_nanos))
    );
}

/// The next whole SSE event, read to the blank line that ends it: an activity
/// event arrives in several body chunks, and chunk boundaries carry no
/// meaning to an SSE client.
pub(crate) async fn next_chunk(body: &mut axum::body::BodyDataStream) -> String {
    let mut event = String::new();
    while !event.ends_with("\n\n") {
        let chunk = tokio::time::timeout(Duration::from_secs(2), body.next())
            .await
            .expect("the expected wall transition is prompt")
            .expect("the stream stays open for the transition")
            .expect("the SSE body is infallible");
        event.push_str(std::str::from_utf8(&chunk).expect("SSE is UTF-8"));
    }
    event
}

pub(crate) async fn stream_ends(body: &mut axum::body::BodyDataStream) -> bool {
    for _frame in 0..3 {
        match body.next().await {
            None => return true,
            Some(Ok(_heartbeat)) => {}
            Some(Err(_infallible)) => return false,
        }
    }
    false
}

pub(crate) struct Fixture {
    pub(crate) lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) tenant: String,
    pub(crate) workspace: Uuid7,
    pub(crate) fleet: String,
    pub(crate) key: String,
    pub(crate) token: String,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        Self::with_pool(&[]).await
    }

    pub(crate) async fn with_pool(settings: &[(&str, &str)]) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, settings).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    pub(crate) async fn seed(&self) {
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

    pub(crate) async fn seed_second_fleet(&self) -> String {
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

    pub(crate) async fn cleanup(self) {
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

/// One live workspace stream and what drives it.
pub(crate) struct Wall {
    pub(crate) fixture: Fixture,
    pub(crate) ownership: OneWorkspace,
    pub(crate) store: Fleets,
    pub(crate) hub: SubscriptionHub,
    pub(crate) body: BodyDataStream,
    pub(crate) publisher: FleetStreams,
}

impl Wall {
    pub(crate) async fn open(fixture: Fixture) -> Self {
        fixture.seed().await;
        let hub = SubscriptionHub::start(harness::dragonfly_config())
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
        let store = fleet.fleet_store();
        let body = open_stream(&fleet.router(), &fixture).await;
        let publisher = FleetStreams::new(
            afd_dragonfly::Dragonfly::connect(&harness::dragonfly_config())
                .await
                .expect("the lane's Dragonfly accepts a publisher"),
        );
        Self {
            fixture,
            ownership,
            store,
            hub,
            body,
            publisher,
        }
    }

    pub(crate) async fn publish(&self, count: usize) {
        let channel = fleet_activity_channel(&self.fixture.fleet);
        for _ in 0..count {
            self.publisher
                .publish(&channel, CHUNK)
                .await
                .expect("the frame publishes");
        }
    }

    /// The wall says nothing for a while.
    pub(crate) async fn stays_quiet(&mut self) {
        let deadline = tokio::time::Instant::now() + QUIET;
        while let Ok(event) = tokio::time::timeout_at(deadline, next_chunk(&mut self.body)).await {
            assert!(
                is_heartbeat(&event),
                "the wall said nothing but heartbeats: {event}"
            );
        }
    }

    /// The next event whose kind is `kind`, skipping activity and the
    /// heartbeats a stream sends whenever its clock passes the interval.
    pub(crate) async fn next_of(&mut self, kind: &str) -> String {
        loop {
            let event = next_chunk(&mut self.body).await;
            if event.contains(&format!("event: {kind}")) {
                return event;
            }
            assert!(
                event.contains("event: chunk") || is_heartbeat(&event),
                "only activity and heartbeats in between: {event}"
            );
        }
    }

    pub(crate) async fn close(self) {
        drop(self.body);
        self.hub.shutdown();
        self.fixture.cleanup().await;
    }
}

/// Whether `event` is the keep-alive a quiet stream sends on its own clock.
fn is_heartbeat(event: &str) -> bool {
    event.contains(&format!("event: {}", afd_sse::HEARTBEAT_EVENT))
}

/// The JSON on an event's `data:` line.
pub(crate) fn data_of(event: &str) -> serde_json::Value {
    let data = event
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .expect("the event carries a data line");
    serde_json::from_str(data.trim()).expect("the event's data is JSON")
}
