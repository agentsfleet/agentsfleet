//! Each Save owns its private draft, wallet and model.

use super::super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use afd_auth::Scope;
use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_core::event::status::RECORDED;
use afd_dragonfly::{
    Dragonfly,
    config::{DragonflyConfig, DragonflyRole},
};
use afd_events::activity::{ACTOR_ACTIVITY, EVENT_TYPE_ACTIVITY};
use axum::Router;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

pub(super) struct Case {
    pub(super) base: Fixture,
    pub(super) router: Router,
    pub(super) queue: Dragonfly,
}

impl Case {
    pub(super) async fn create() -> Self {
        Self::with_budgets(afd_admission::Budgets::default()).await
    }

    pub(super) async fn with_budgets(budgets: afd_admission::Budgets) -> Self {
        let base = Fixture::create().await;
        let config = DragonflyConfig::from_url(
            DragonflyRole::Default,
            ProcessEnv
                .get("TEST_DRAGONFLY_URL")
                .expect("live cluster URL"),
        );
        let queue = afd_dragonfly::test_util::connect_live(&config)
            .await
            .expect("live cluster");
        let router = base
            .services(&Scope::ALL)
            .with_fleet_queue_budgets(base.db.clone(), queue.clone(), budgets)
            .router();
        Self {
            base,
            router,
            queue,
        }
    }

    pub(super) fn collection(&self) -> String {
        format!("/v1/workspaces/{}/fleets", self.base.workspace)
    }

    pub(super) async fn request(&self) -> (String, Value) {
        self.request_with(json!({})).await
    }

    pub(super) async fn request_with(&self, config: Value) -> (String, Value) {
        let mut body = self.base.request(&mint());
        body.as_object_mut().expect("request").remove("message");
        body["source"]["definition"]["config"]["x-agentsfleet"]
            .as_object_mut()
            .expect("runtime config")
            .extend(config.as_object().expect("config amendments").clone());
        let response = send(
            &self.router,
            Method::POST,
            &self.base.collection(),
            Some(&self.base.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("private location")
            .to_owned();
        let tag = response.headers()[header::ETAG]
            .to_str()
            .expect("draft tag")
            .to_owned();
        assert!(
            json_body(response).await.get("review").is_some(),
            "creation includes the current review"
        );
        let patch = json!({"runtime_model_ref":self.base.model,"instance_name":"http-chat"});
        let response = send_with_headers(
            &self.router,
            Method::PATCH,
            &item,
            Some(&self.base.token),
            &patch.to_string(),
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let draft = json_body(response).await;
        assert_eq!(
            draft.pointer("/review/effective/instance_name"),
            Some(&json!("http-chat"))
        );
        (
            item,
            json!({"source":{"kind":"draft","draft_candidate_id":draft["candidate"]["id"]},
            "name":"http-chat", "review_digest":draft["review"]["review_digest"],"start":{"kind":"none"}}),
        )
    }

    pub(super) async fn fund(&self) {
        sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
            .bind(self.base.tenant.as_str()).bind(crate::authoring::FUNDED_NANOS)
            .execute(&mut *self.base.db.acquire().await.expect("wallet connection")).await.expect("fund wallet");
    }

    pub(super) async fn save(&self, key: &str, body: &Value) -> axum::response::Response {
        send_with_headers(
            &self.router,
            Method::POST,
            &self.collection(),
            Some(&self.base.token),
            &body.to_string(),
            &[(header::HeaderName::from_static("idempotency-key"), key)],
        )
        .await
    }

    pub(super) async fn cleanup(self) {
        self.base.cleanup().await;
    }
}

pub(super) fn runtime_history(page: &Value, fleet: &str, workspace: &str) -> Vec<Value> {
    history_identities(page, fleet, workspace);
    let rows = page["items"].as_array().expect("complete history page");
    let mut receipts = Vec::new();
    let mut runtime = Vec::new();
    for row in rows {
        if row["event_type"] == EVENT_TYPE_ACTIVITY {
            assert_eq!(row["actor"], ACTOR_ACTIVITY);
            assert_eq!(row["status"], RECORDED);
            assert!(row["response_text"].is_null());
            receipts.push(
                serde_json::from_str::<Value>(row["request_json"].as_str().expect("receipt JSON"))
                    .expect("safe receipt metadata"),
            );
        } else {
            runtime.push(row.clone());
        }
    }
    receipts.sort_by_key(Value::to_string);
    // pin test: literal is the contract
    assert_eq!(
        receipts,
        vec![
            json!({"kind":"setup_ready", "revision":"1"}),
            json!({"kind":"setup_saved", "revision":"1"}),
        ]
    );
    runtime
}

pub(super) fn history_identities(page: &Value, fleet: &str, workspace: &str) -> Vec<String> {
    let rows = page["items"].as_array().expect("complete history page");
    assert!(
        page["next_cursor"].is_null(),
        "the entire history is inspected"
    );
    let mut identities = std::collections::BTreeSet::new();
    for row in rows {
        assert_eq!(row["fleet_id"], fleet);
        assert_eq!(row["workspace_id"], workspace);
        let id = row["event_id"].as_str().expect("event identity");
        assert!(
            identities.insert(id.to_owned()),
            "unique history identities"
        );
    }
    identities.into_iter().collect()
}
