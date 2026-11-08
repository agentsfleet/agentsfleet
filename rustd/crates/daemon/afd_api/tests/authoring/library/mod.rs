//! Library routes preserve publication intent, immutable history and tenant bounds.

mod foreign;
mod pagination;
mod platform;
mod publication;
mod removal;

use super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use axum::{Router, response::Response};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

struct Case {
    base: Fixture,
    router: Router,
    draft: String,
    candidate: Value,
}

impl Case {
    async fn create() -> Self {
        let base = Fixture::create().await;
        let router = base.router();
        let mut request = base.request(&mint());
        request.as_object_mut().expect("request").remove("message");
        let response = send(
            &router,
            Method::POST,
            &base.collection(),
            Some(&base.token),
            &request.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let draft = response.headers()[header::LOCATION]
            .to_str()
            .expect("draft location")
            .to_owned();
        let value = json_body(response).await;
        Self {
            base,
            router,
            draft,
            candidate: value["candidate"]["id"].clone(),
        }
    }

    fn collection(&self) -> String {
        format!("/v1/workspaces/{}/fleet-libraries", self.base.workspace)
    }

    async fn publish(&self, key: &str) -> Response {
        send_with_headers(
            &self.router,
            Method::POST,
            &self.collection(),
            Some(&self.base.token),
            &json!({"draft_candidate_id":self.candidate,"label":"ARGO"}).to_string(),
            &[(header::HeaderName::from_static("idempotency-key"), key)],
        )
        .await
    }

    async fn get(&self, path: &str) -> Response {
        send(&self.router, Method::GET, path, Some(&self.base.token), "").await
    }

    async fn edit(&mut self) {
        let current = self.get(&self.draft).await;
        let tag = current.headers()[header::ETAG]
            .to_str()
            .expect("current draft tag")
            .to_owned();
        let response = send_with_headers(
            &self.router,
            Method::PATCH,
            &self.draft,
            Some(&self.base.token),
            r#"{"instruction_edits":[{"start_byte":0,"end_byte":0,"text":"Continue.\n"}]}"#,
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        self.candidate = json_body(response).await["candidate"]["id"].clone();
    }
}
