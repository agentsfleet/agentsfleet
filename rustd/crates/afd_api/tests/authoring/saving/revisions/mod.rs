//! Improvement drafts target one immutable setup, independently of resource edits.

mod apply;
mod policy;
mod purge;
mod refusals;
mod schedules;

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

struct Fleet {
    case: Case,
    item: String,
    saved: Value,
}

impl Fleet {
    async fn create() -> Self {
        let case = Case::create().await;
        case.fund().await;
        let (_, request) = case.request().await;
        let response = case.save(mint().as_str(), &request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("fleet location")
            .to_owned();
        let saved = json_body(response).await;
        assert_eq!(saved["status"], "active");
        Self { case, item, saved }
    }

    async fn improvement(&self) -> (String, Value) {
        self.improvement_of(&self.saved["setup_revision"]).await
    }

    /// A private improvement draft copied from setup `revision`, edited once
    /// and reviewed. Answers its location and the Apply body for that review.
    async fn improvement_of(&self, revision: &Value) -> (String, Value) {
        let request = json!({"source":{"kind":"fleet","fleet_id":self.saved["id"],"setup_revision":revision},
            "authoring_model_ref":self.case.base.model,"operation_id":mint()});
        let response = send(
            &self.case.router,
            Method::POST,
            &self.case.base.collection(),
            Some(&self.case.base.token),
            &request.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let draft = response.headers()[header::LOCATION]
            .to_str()
            .expect("improvement draft")
            .to_owned();
        let tag = response.headers()[header::ETAG]
            .to_str()
            .expect("draft tag")
            .to_owned();
        let response = send_with_headers(
            &self.case.router,
            Method::PATCH,
            &draft,
            Some(&self.case.base.token),
            r#"{"instruction_edits":[{"start_byte":0,"end_byte":0,"text":"Explain the regression.\n"}]}"#,
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let current = json_body(response).await;
        (
            draft,
            json!({"draft_candidate_id":current["candidate"]["id"],"review_digest":current["review"]["review_digest"]}),
        )
    }

    async fn apply(&self, key: &str, tag: &str, request: &Value) -> axum::response::Response {
        send_with_headers(
            &self.case.router,
            Method::POST,
            &format!("{}/revisions", self.item),
            Some(&self.case.base.token),
            &request.to_string(),
            &[
                (header::IF_MATCH, tag),
                (header::HeaderName::from_static("idempotency-key"), key),
            ],
        )
        .await
    }

    /// One `PATCH` of the fleet resource (name or status) under `tag`.
    async fn patch_resource(&self, body: &str, tag: &str) -> axum::response::Response {
        send_with_headers(
            &self.case.router,
            Method::PATCH,
            &self.item,
            Some(&self.case.base.token),
            body,
            &[(header::IF_MATCH, tag)],
        )
        .await
    }

    async fn current(&self) -> Value {
        let response = send(
            &self.case.router,
            Method::GET,
            &self.item,
            Some(&self.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        json_body(response).await
    }

    async fn revision_count(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM core.fleet_definition_revisions WHERE fleet_id=$1::uuid",
        )
        .bind(self.saved["id"].as_str().expect("fleet ID"))
        .fetch_one(
            &mut *self
                .case
                .base
                .db
                .acquire()
                .await
                .expect("revision connection"),
        )
        .await
        .expect("revision count")
    }
}
