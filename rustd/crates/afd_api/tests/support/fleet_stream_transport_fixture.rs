//! Fixture-local pools and channels keep transport faults away from sibling tests.

use std::time::Duration;

use afd_wire::report::Outcome;
use afd_wire::tail::{FleetCounters, TailFrame, TailRow};

use afd_auth::scope::{Scope, ScopeSet};
use afd_dragonfly::streams::{FleetStreams, fleet_activity_channel};
use afd_dragonfly::{Dragonfly, SubscriptionHub};
use futures_util::StreamExt as _;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::super::{Fixture, SUBJECT};
use crate::harness::{self, Fleet};

pub(super) const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// The sequence a control frame rides.
///
/// Zero, and deliberately not a number from the connection's counter:
/// `hello` is the server talking ABOUT the stream, so burning a sequence
/// on it would leave a gap in the ids a client uses to tell a dropped frame
/// from a control one. Mirrors `afd_sse::frame`'s own `SYNTHETIC_SEQ`.
const HELLO_SEQ: u64 = 0;
const MAX_VIEWERS: usize = 100;

pub(super) struct Watched {
    fixture: Fixture,
    router: axum::Router,
    publisher: FleetStreams,
    pub(super) hub: SubscriptionHub,
}

impl Watched {
    pub(super) async fn create() -> Self {
        let fixture = Fixture::with_pool(&[
            ("DATABASE_POOL_SIZE_API", "1"),
            ("DATABASE_MIN_POOL_SIZE_API", "1"),
        ])
        .await;
        fixture.seed().await;
        let hub = SubscriptionHub::start(harness::dragonfly_config())
            .await
            .expect("live hub");
        let publisher = FleetStreams::new(
            Dragonfly::connect(&harness::dragonfly_config())
                .await
                .expect("publisher"),
        );
        let router = Fleet::live(
            fixture.database.clone(),
            SUBJECT,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(fixture.workspace.clone())
        .with_stream_capacity(hub.clone(), MAX_VIEWERS)
        .router();
        Self {
            fixture,
            router,
            publisher,
            hub,
        }
    }

    pub(super) fn database(&self) -> &afd_db::Db {
        &self.fixture.database
    }

    pub(super) fn channel(&self) -> String {
        fleet_activity_channel(&self.fixture.fleet)
    }

    fn events_path(&self) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/events",
            self.fixture.workspace, self.fixture.fleet
        )
    }

    pub(super) async fn open(&self) -> axum::body::BodyDataStream {
        let response = harness::send(
            &self.router,
            Method::GET,
            &format!("{}/stream", self.events_path()),
            Some(&self.fixture.token),
            "",
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the admitted stream opens"
        );
        assert_eq!(
            response
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        response.into_body().into_data_stream()
    }

    pub(super) async fn ready(&self, bodies: &mut [axum::body::BodyDataStream]) {
        let payload = json!({"kind":"ready"});
        tokio::time::timeout(DELIVERY_BUDGET, async {
            loop {
                if self.publish(&payload).await == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Dragonfly acknowledges the first server-side subscription");
        for body in bodies {
            // The route announces itself before any activity, so every body
            // opens with `hello` and this barrier's own payload is the SECOND
            // frame. Asserted rather than skipped: a fixture that silently
            // swallowed a frame would hide the opening frame going missing,
            // and the opening frame is what tells a client watching a quiet
            // fleet that its subscription attached.
            assert_frame(
                &next_frame(body).await,
                HELLO_SEQ,
                &json!({"kind":"hello","fleet_ids":[&self.fixture.fleet],"counters":{}}),
            );
            // Still sequence zero: `hello` is the server talking about the
            // stream, so it spends no activity number.
            assert_frame(&next_frame(body).await, 0, &payload);
        }
    }

    pub(super) async fn publish(&self, payload: &Value) -> i64 {
        self.publisher
            .publish(&self.channel(), &payload.to_string())
            .await
            .expect("publish succeeds")
    }

    pub(super) async fn unsubscribed(&self) {
        assert_eq!(self.hub.readers(&self.channel()), 0);
        tokio::time::timeout(DELIVERY_BUDGET, async {
            while self.publish(&json!({"kind":"closed"})).await != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Dragonfly releases the last subscription after the body drops");
    }

    pub(super) async fn commit_without_publish(&self) -> String {
        let event_id = afd_db::test_util::mint_id();
        sqlx::query(
            "INSERT INTO core.fleet_events \
            (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, \
             response_text, tokens, wall_ms, created_at, updated_at) \
            VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', $4, \
                    '{}', 'recovered answer', 7, 12, 10, 11)",
        )
        .bind(&self.fixture.fleet)
        .bind(self.fixture.workspace.as_str())
        .bind(&event_id)
        .bind(Outcome::Processed.as_str())
        .execute(
            self.database()
                .acquire()
                .await
                .expect("durable write")
                .as_mut(),
        )
        .await
        .expect("the history row commits without publishing");
        event_id
    }

    pub(super) async fn history(&self) -> Value {
        let response = harness::send(
            &self.router,
            Method::GET,
            &format!("{}?limit=20", self.events_path()),
            Some(&self.fixture.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        harness::json_body(response).await
    }

    pub(super) async fn cleanup(self) {
        self.hub.shutdown();
        drop(self.router);
        drop(self.publisher);
        self.fixture.cleanup().await;
    }
}

pub(super) async fn next_frame(body: &mut axum::body::BodyDataStream) -> String {
    let bytes = tokio::time::timeout(DELIVERY_BUDGET, body.next())
        .await
        .expect("frame delivered within budget")
        .expect("stream stays open")
        .expect("infallible SSE body");
    String::from_utf8(bytes.to_vec()).expect("SSE is UTF-8")
}

pub(super) fn assert_frame(frame: &str, sequence: u64, payload: &Value) {
    let field = |name: &str| {
        frame
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
    };
    assert_eq!(field("id:"), Some(sequence.to_string().as_str()));
    assert_eq!(field("event:"), payload.get("kind").and_then(Value::as_str));
    let decoded: Value =
        serde_json::from_str(field("data:").expect("frame data")).expect("JSON payload");
    assert_eq!(&decoded, payload);
}

/// The activity bridge's private `Published::Chunk` names the pub/sub kind;
/// `ActivityFrame::FleetResponseChunk` names a different runner HTTP payload.
pub(super) fn chunk(event_id: &str, text: &str) -> Value {
    const CHUNK_KIND: &str = "chunk";
    json!({"kind": CHUNK_KIND, "event_id": event_id, "text": text})
}

pub(super) fn completion(event_id: &str) -> Value {
    serde_json::to_value(TailFrame::EventComplete {
        event: Box::new(TailRow {
            event_id: event_id.into(),
            actor: "steer:api".into(),
            event_type: "chat".into(),
            status: Outcome::Processed.as_str().into(),
            tokens: Some(7),
            wall_ms: Some(12),
            failure_label: None,
            failure_detail: None,
            checkpoint_id: None,
            resumes_event_id: None,
            created_at: 10,
            updated_at: 11,
            cost_nanos: None,
        }),
        final_reply: None,
        fleet_status: afd_fleet_lifecycle::FleetStatus::Active.as_str().into(),
        pending_approvals: 0,
        counters: Some(FleetCounters {
            events_processed: 3,
            budget_used_nanos: 21,
        }),
    })
    .expect("the canonical completion frame serializes")
}
