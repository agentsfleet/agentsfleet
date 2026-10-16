//! A fleet created through Save serves messages, history and grants over the
//! live stores.
//!
//! The fleet comes from the same draft, review and Save requests a browser or
//! terminal client sends, so nothing here seeds a fleet row by hand. What the
//! runtime would leave behind — one completed event and one approved grant —
//! is seeded, because producing either needs a runner and a person's answer.

mod message;

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
use afd_auth::Scope;
use afd_core::id::Uuid7;
use axum::Router;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

/// The completed event every runtime case reads back.
const EVENT: &str = "1760000000000-0";

/// One saved, active fleet and a router that also admits messages.
struct Saved {
    case: Case,
    router: Router,
    workspace: String,
    fleet: Uuid7,
    item: String,
    thread: String,
}

impl Saved {
    async fn create() -> Self {
        let case = Case::create().await;
        case.fund().await;
        let (_, request) = case.request().await;
        let saved = case.save(mint().as_str(), &request).await;
        assert_eq!(saved.status(), StatusCode::CREATED);
        let item = saved.headers()[header::LOCATION]
            .to_str()
            .expect("fleet location")
            .to_owned();
        let body = json_body(saved).await;
        assert_eq!(body["status"], "active", "{body}");
        let fleet = Uuid7::parse(body["id"].as_str().expect("fleet id")).expect("canonical id");
        let db = case.base.db.clone();
        let router = case
            .base
            .services(&Scope::ALL)
            .with_fleet_queue(db.clone(), case.queue.clone())
            .with_steering_queue(db, case.queue.clone())
            .router();
        let workspace = format!("/v1/workspaces/{}", case.base.workspace.as_str());
        let thread = format!("{item}/messages");
        Self {
            case,
            router,
            workspace,
            fleet,
            item,
            thread,
        }
    }

    fn token(&self) -> &str {
        &self.case.base.token
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = send(&self.router, Method::GET, path, Some(self.token()), "").await;
        let status = response.status();
        (status, json_body(response).await)
    }

    /// One message naming `operation`, answered with the status and the body.
    async fn steer(&self, thread: &str, operation: &str, message: &str) -> (StatusCode, Value) {
        let body = json!({"message": message, "operation_id": operation}).to_string();
        let response = send(
            &self.router,
            Method::POST,
            thread,
            Some(self.token()),
            &body,
        )
        .await;
        let status = response.status();
        (status, json_body(response).await)
    }

    /// Stops the fleet against its current tag; a stopped fleet meets the same
    /// ingress refusal the anomaly gate's pause does.
    async fn stop(&self) {
        let current = send(
            &self.router,
            Method::GET,
            &self.item,
            Some(self.token()),
            "",
        )
        .await;
        let tag = current.headers()[header::ETAG]
            .to_str()
            .expect("fleet tag")
            .to_owned();
        let stopped = send_with_headers(
            &self.router,
            Method::PATCH,
            &self.item,
            Some(self.token()),
            r#"{"status":"stopped"}"#,
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(stopped.status(), StatusCode::OK);
    }

    /// The completed event and the approved grant a run and a person leave.
    async fn seed_event_and_grant(&self, grant: &Uuid7) {
        let mut connection = self.case.base.db.acquire().await.expect("a connection");
        sqlx::query(
            "INSERT INTO core.fleet_events (fleet_id, workspace_id, event_id, actor, event_type, \
             status, request_json, response_text, tokens, wall_ms, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', 'completed', \
             '{\"prompt\":\"ship\"}', 'done', 7, 12, 10, 11)",
        )
        .bind(self.fleet.as_str())
        .bind(self.case.base.workspace.as_str())
        .bind(EVENT)
        .execute(&mut *connection)
        .await
        .expect("the completed event seeds");
        sqlx::query(
            "INSERT INTO core.integration_grants \
             (id, fleet_id, service, status, requested_reason, approved_at, created_at) \
             VALUES ($1::uuid, $2::uuid, 'github', 'approved', 'fixture', 10, 9)",
        )
        .bind(grant.as_str())
        .bind(self.fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the approved grant seeds");
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_saved_fleet_serves_messages_history_and_grants() {
    let saved = Saved::create().await;
    let grant = mint();
    saved.seed_event_and_grant(&grant).await;
    assert_thread_lists_the_event_and_admits_a_message(&saved).await;
    assert_history_serves_the_event(&saved).await;
    assert_grant_lists_and_revokes_once(&saved, &grant).await;
    saved.case.cleanup().await;
}

/// The fleet's message thread lists the seeded event and accepts a new message.
async fn assert_thread_lists_the_event_and_admits_a_message(saved: &Saved) {
    let (status, listed) = saved.get(&saved.thread).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.pointer("/items/0/event_id"), Some(&json!(EVENT)));
    let (status, steered) = saved
        .steer(&saved.thread, mint().as_str(), "ship the next change")
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{steered}");
    assert_eq!(steered["status"], "accepted");
}

/// The workspace page, the fleet's filtered page and the event detail all
/// serve the seeded event, its request JSON intact.
async fn assert_history_serves_the_event(saved: &Saved) {
    let workspace_page = format!(
        "{}/events?fleet_id={}",
        saved.workspace,
        saved.fleet.as_str()
    );
    let (status, page) = saved.get(&workspace_page).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page.pointer("/items/0/event_id"), Some(&json!(EVENT)));
    let events = format!("{}/events", saved.item);
    let (status, page) = saved.get(&format!("{events}?actor_prefix=steer")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));
    let (status, detail) = saved.get(&format!("{events}/{EVENT}")).await;
    assert_eq!(status, StatusCode::OK);
    let request = detail["request_json"]
        .as_str()
        .expect("opaque request JSON");
    assert_eq!(
        serde_json::from_str::<Value>(request).expect("stored request JSON stays valid"),
        json!({"prompt":"ship"})
    );
}

/// The seeded grant lists, revokes once, and is gone on the second revoke.
async fn assert_grant_lists_and_revokes_once(saved: &Saved, grant: &Uuid7) {
    let grants = format!("{}/integration-grants", saved.item);
    let (status, listed) = saved.get(&grants).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.pointer("/items/0/id"), Some(&json!(grant.as_str())));
    let revoke = format!("{grants}/{}", grant.as_str());
    for expected in [StatusCode::NO_CONTENT, StatusCode::NOT_FOUND] {
        let revoked = send(
            &saved.router,
            Method::DELETE,
            &revoke,
            Some(saved.token()),
            "",
        )
        .await;
        assert_eq!(revoked.status(), expected);
    }
}
