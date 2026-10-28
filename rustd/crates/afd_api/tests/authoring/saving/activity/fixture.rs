//! Every case owns its account; routers can be rebuilt without losing durable history.

use super::super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::Case,
};
use afd_core::id::Uuid7;
use afd_db::test_util::{FIXTURE_SETUP_SEMANTICS, FleetSeed, SetupRevision};
use afd_fleet_runtime::{Definition, definition::canonical_digest};
use axum::{Router, response::Response};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

pub(super) const SERVICE: &str = "receipt_access";

pub(super) struct Saved {
    pub case: Case,
    pub item: String,
    pub fleet: Uuid7,
    pub initial: Value,
}

impl Saved {
    pub(super) async fn create(config: Value) -> Self {
        let case = Case::create().await;
        case.fund().await;
        let (_, request) = case.request_with(config).await;
        let response = case.save(mint().as_str(), &request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("fleet location")
            .to_owned();
        let initial = json_body(response).await;
        let fleet = Uuid7::parse(initial["id"].as_str().expect("fleet ID")).expect("fleet UUID");
        Self {
            case,
            item,
            fleet,
            initial,
        }
    }

    pub(super) fn reloaded_router(&self) -> Router {
        self.case.base.router()
    }

    pub(super) async fn get(&self, suffix: &str) -> Value {
        let response = send(
            &self.reloaded_router(),
            Method::GET,
            &format!("{}{suffix}", self.item),
            Some(&self.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        json_body(response).await
    }

    pub(super) async fn receipts(&self) -> Vec<Value> {
        self.receipts_for(&self.fleet).await
    }

    pub(super) async fn receipts_for(&self, fleet: &Uuid7) -> Vec<Value> {
        let path = format!(
            "/v1/workspaces/{}/fleets/{fleet}/messages",
            self.case.base.workspace
        );
        let response = send(
            &self.reloaded_router(),
            Method::GET,
            &path,
            Some(&self.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let history = json_body(response).await;
        let rows = history["items"].as_array().expect("history items");
        rows.iter()
            .map(|row| {
                assert_eq!(row["fleet_id"], fleet.as_str());
                assert_eq!(row["workspace_id"], self.case.base.workspace.as_str());
                assert_eq!(row["event_type"], "activity");
                assert_eq!(row["actor"], "system");
                assert_eq!(row["status"], "recorded");
                assert!(row["response_text"].is_null());
                serde_json::from_str(row["request_json"].as_str().expect("receipt JSON"))
                    .expect("receipt payload")
            })
            .collect()
    }

    pub(super) async fn access(&self, method: Method, value: &str) -> Response {
        let suffix = if method == Method::POST {
            String::new()
        } else {
            format!("/{SERVICE}")
        };
        let path = format!(
            "/v1/workspaces/{}/secrets{suffix}",
            self.case.base.workspace
        );
        let data = json!({"api_key": value});
        let body = if method == Method::POST {
            json!({"name": SERVICE, "data": data})
        } else {
            json!({"data": data})
        };
        send(
            &self.case.router,
            method,
            &path,
            Some(&self.case.base.token),
            &body.to_string(),
        )
        .await
    }

    pub(super) async fn memory(&self, read: bool, publish: bool) -> Response {
        send(
            &self.case.router,
            Method::PATCH,
            &format!("{}/memory-access", self.item),
            Some(&self.case.base.token),
            &json!({"read":read,"publish":publish}).to_string(),
        )
        .await
    }

    pub(super) async fn activate(&self) -> Response {
        let current = self.get("").await;
        send_with_headers(
            &self.case.router,
            Method::PATCH,
            &self.item,
            Some(&self.case.base.token),
            r#"{"status":"active"}"#,
            &[(
                header::IF_MATCH,
                current["etag"].as_str().expect("fresh resource tag"),
            )],
        )
        .await
    }

    pub(super) async fn grants(&self) -> Value {
        self.get("").await["memory_access"].clone()
    }

    /// Seed another saved fleet without emitting the receipts under test.
    pub(super) async fn neighbor(&self, name: &str, config: Value) -> Uuid7 {
        let id = mint();
        let mut definition = self.initial["definition"].clone();
        definition["config"]["x-agentsfleet"]
            .as_object_mut()
            .expect("config")
            .extend(config.as_object().expect("config amendments").clone());
        let definition = Definition::parse(&definition.to_string()).expect("valid neighbor setup");
        let digest = canonical_digest(&definition).expect("neighbor setup digest");
        let definition = serde_json::to_string(&definition).expect("canonical neighbor setup");
        FleetSeed::new(
            id.as_str(),
            self.case.base.workspace.as_str(),
            self.case.base.tenant.as_str(),
            "active",
        )
        .name(name)
        .unplaced_setup(SetupRevision {
            definition: &definition,
            digest: &digest,
            semantics: FIXTURE_SETUP_SEMANTICS,
            model_ref: Some(self.case.base.model.as_str()),
        })
        .insert(&mut *self.case.base.db.acquire().await.expect("seed connection"))
        .await
        .expect("neighbor fleet");
        id
    }
}

pub(super) fn saved_receipt() -> Value {
    json!({"kind":"setup_saved", "revision":"1"})
}

pub(super) fn ready_receipt() -> Value {
    json!({"kind":"setup_ready", "revision":"1"})
}

pub(super) fn access_receipt() -> Value {
    json!({"kind":"access_saved", "service":SERVICE})
}

pub(super) fn memory_receipt(read: bool, publish: bool) -> Value {
    json!({"kind":"memory_sharing_changed", "read":read, "publish":publish})
}

pub(super) fn assert_receipts(actual: &[Value], expected: &[Value]) {
    let sorted = |values: &[Value]| {
        let mut values: Vec<_> = values.iter().map(Value::to_string).collect();
        values.sort();
        values
    };
    assert_eq!(
        sorted(actual),
        sorted(expected),
        "exact safe metadata after fresh HTTP reads"
    );
}
