//! Each Save owns its private draft, wallet, model and shared fleet stream.

use super::super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use afd_auth::Scope;
use afd_core::{
    clock,
    env::{EnvSource as _, ProcessEnv},
};
use afd_dragonfly::{
    Dragonfly, FleetStreams, ReadyIndex,
    config::{DragonflyConfig, DragonflyRole},
};
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
        let mut body = self.base.request(&mint());
        body.as_object_mut().expect("request").remove("message");
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
        sqlx::query("INSERT INTO billing.tenant_wallet(tenant_id,balance_nanos,grant_source,created_at,updated_at) VALUES($1::uuid,$3,'http-save',$2,$2)")
            .bind(self.base.tenant.as_str()).bind(clock::now().as_millis()).bind(crate::authoring::FUNDED_NANOS)
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
        let fleets: Vec<String> =
            sqlx::query_scalar("SELECT id::text FROM core.fleets WHERE workspace_id=$1::uuid")
                .bind(self.base.workspace.as_str())
                .fetch_all(&mut *self.base.db.acquire().await.expect("cleanup connection"))
                .await
                .expect("owned fleets");
        for fleet in fleets {
            FleetStreams::new(self.queue.clone())
                .forget(&fleet)
                .await
                .expect("remove own stream");
            ReadyIndex::new(self.queue.clone())
                .force_clear(&fleet)
                .await
                .expect("remove own hint");
        }
        self.base.cleanup().await;
    }
}
