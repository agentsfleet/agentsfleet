//! Real authorization and durable state; the detached server supplies network pushes.

use super::super::{fixture::Fixture, json_body, mint, send};
use afd_auth::scope::Scope;
use afd_core::id::Uuid7;
use afd_dragonfly::{SubscriptionHub, hub::DetachedServer};
use axum::{Router, body::BodyDataStream, response::Response};
use futures_util::StreamExt as _;
use http::{Method, StatusCode};
use serde_json::{Value, json};

pub(super) struct Live {
    pub(super) f: Fixture,
    pub(super) router: Router,
    pub(super) draft: Uuid7,
    pub(super) item: String,
    pub(super) hub: SubscriptionHub,
    pub(super) server: DetachedServer,
}

impl Live {
    pub(super) async fn create(capacity: usize) -> Self {
        let f = Fixture::create().await;
        let (hub, server) = SubscriptionHub::detached();
        let router = f
            .services(&Scope::ALL)
            .with_stream_capacity(hub.clone(), capacity)
            .router();
        let created = send(
            &router,
            Method::POST,
            &f.collection(),
            Some(&f.token),
            &f.request(&mint()).to_string(),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let item = created.headers()[http::header::LOCATION]
            .to_str()
            .expect("location")
            .to_owned();
        let document = json_body(created).await;
        let draft = Uuid7::parse(document["id"].as_str().expect("draft")).expect("draft UUIDv7");
        sqlx::query("UPDATE authoring.messages SET status='running' WHERE draft_id=$1::uuid")
            .bind(draft.as_str())
            .execute(&mut *f.db.acquire().await.expect("message connection"))
            .await
            .expect("leased message state");
        Self {
            f,
            router,
            draft,
            item,
            hub,
            server,
        }
    }

    pub(super) async fn response(&self) -> Response {
        send(
            &self.router,
            Method::GET,
            &format!("{}/events/stream", self.item),
            Some(&self.f.token),
            "",
        )
        .await
    }

    pub(super) async fn open(&self) -> BodyDataStream {
        let response = self.response().await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/event-stream"
        );
        let mut body = response.into_body().into_data_stream();
        assert!(frame(&mut body).await.contains("event: message.running\n"));
        assert!(
            frame(&mut body)
                .await
                .contains("event: candidate.changed\n")
        );
        self.server.confirm(&self.channel());
        body
    }

    pub(super) fn channel(&self) -> String {
        afd_authoring::draft_channel(&self.draft)
    }

    pub(super) fn delta(&self, text: &str) -> Value {
        json!({"draft_id":self.draft,"sequence":"1","stream_id":mint(),
            "stream_seq":1,"stream_start":true,"text_kind":"answer","text":text})
    }

    pub(super) fn publish(&self, frame: &Value) {
        self.server.publish(&self.channel(), &frame.to_string());
    }

    pub(super) async fn mutate(&self, statement: &'static str) {
        sqlx::query(statement)
            .bind(self.draft.as_str())
            .execute(&mut *self.f.db.acquire().await.expect("mutation connection"))
            .await
            .expect("fixture state transition");
    }

    pub(super) async fn cleanup(self) {
        self.hub.shutdown();
        drop(self.router);
        self.f.cleanup().await;
    }
}

pub(super) async fn frame(body: &mut BodyDataStream) -> String {
    let bytes = tokio::time::timeout(super::DELIVERY, body.next())
        .await
        .expect("private frame deadline")
        .expect("stream remains open")
        .expect("infallible body");
    String::from_utf8(bytes.to_vec()).expect("UTF-8 frame")
}

pub(super) async fn closed(body: &mut BodyDataStream) {
    assert!(
        tokio::time::timeout(super::DELIVERY, body.next())
            .await
            .expect("private close deadline")
            .is_none()
    );
}

pub(super) fn data(frame: &str) -> Value {
    serde_json::from_str(
        frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("event data"),
    )
    .expect("JSON event")
}
