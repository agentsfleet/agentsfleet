//! Native HTTP envelopes are observed on the socket, including original-owner receipts.
use super::{CALL, FENCING, fixture};
use crate::{
    client::{ControlPlane, HttpRunnerApi},
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN},
};
use axum::{
    body::Bytes,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<(String, Value)>>>;
pub(super) struct Server {
    pub(super) plane: ControlPlane,
    calls: Calls,
    native: crate::test_support::Calls,
    mode: &'static str,
    worker: tokio::task::JoinHandle<()>,
}
impl Server {
    pub(super) async fn start(mode: &'static str) -> Self {
        let calls: Calls = Arc::new(Mutex::new(vec![]));
        let owned = Arc::clone(&calls);
        let app = axum::Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let owned = Arc::clone(&owned);
                async move { respond(mode, &owned, &method, &uri, &headers, &body) }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let env = af_core::env::MapEnv::from_pairs([
            (ENV_API_URL, base.as_str()),
            (ENV_RUNNER_TOKEN, "agt_r_decision_fixture"),
            (crate::config::ENV_STREAM_URL, "https://localhost:4443"),
        ]);
        let api = HttpRunnerApi::new(&Config::from_env(&env).unwrap()).unwrap();
        let native_calls = Arc::clone(&calls);
        let (plane, native) = crate::test_support::plane_with_http(Box::new(api), move |call| {
            let body = call
                .body
                .as_ref()
                .map_or(Value::Null, |bytes| serde_json::from_slice(bytes).unwrap());
            native_calls
                .lock()
                .unwrap()
                .push((call.path.to_string(), body));
            crate::test_support::json(&crate::test_support::healthy().to(Some(call.verb)))
        });
        Self {
            plane,
            calls,
            native,
            mode,
            worker,
        }
    }
    pub(super) async fn assert_calls(&mut self) {
        if matches!(
            self.mode,
            "empty_credential" | "invalid_header" | "mismatched_receipt"
        ) {
            let wanted = if self.mode == "mismatched_receipt" {
                1
            } else {
                2
            };
            crate::test_support::bounded("native decision progress", async {
                loop {
                    let observed = self
                        .calls
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(_, body)| body.get("frames").is_some())
                        .count();
                    if observed >= wanted {
                        break;
                    }
                    self.native.recv().await.unwrap();
                }
            })
            .await;
        }
        let calls = self.calls.lock().unwrap();
        let reservations: Vec<_> = calls
            .iter()
            .filter(|(path, _)| path.ends_with("/tasks/decisions"))
            .collect();
        assert_eq!(reservations.len(), 1);
        let records: Vec<_> = calls
            .iter()
            .filter(|(path, _)| path.contains("/tasks/decisions/"))
            .collect();
        let dispatch = matches!(
            self.mode,
            "empty_credential" | "invalid_header" | "mismatched_receipt"
        );
        assert_eq!(records.len(), usize::from(dispatch));
        if dispatch {
            let record = &records.first().unwrap().1;
            assert_eq!(record.get("fencing_token"), Some(&json!(FENCING)));
            assert_eq!(record.pointer("/receipt/outcome"), Some(&json!("not_sent")));
            assert_eq!(record.pointer("/receipt/cause"), Some(&json!("preflight")));
            let started = calls
                .iter()
                .filter(|(_, body)| body.pointer("/frames/0/task_decision_started").is_some())
                .count();
            assert_eq!(started, 1);
            let completed = calls
                .iter()
                .filter(|(_, body)| body.pointer("/frames/0/task_decision_completed").is_some())
                .count();
            assert_eq!(completed, usize::from(self.mode != "mismatched_receipt"));
        }
    }
    pub(super) fn stop(self) {
        self.worker.abort();
    }
}

fn respond(
    mode: &str,
    calls: &Calls,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    bytes: &Bytes,
) -> (StatusCode, axum::Json<Value>) {
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer agt_r_decision_fixture"
    );
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes).unwrap()
    };
    calls
        .lock()
        .unwrap()
        .push((uri.path().into(), body.clone()));
    let reply = if uri.path().ends_with("/inputs") {
        assert_eq!(method, &Method::GET);
        assert!(
            uri.query()
                .unwrap()
                .contains(&format!("fencing_token={FENCING}"))
        );
        serde_json::to_value(fixture::inputs()).unwrap()
    } else if uri.path().ends_with("/effects") {
        json!({"effects":[],"next_cursor":null})
    } else if uri.path().ends_with("/tasks/decisions") {
        assert_eq!(method, &Method::POST);
        let input: af_wire::task::DecisionReservationInput = serde_json::from_value(body).unwrap();
        assert_eq!(input.fencing_token, FENCING);
        assert_eq!(input.source_admission_id, CALL);
        assert!(input.retry_of.is_none());
        fixture::reservation(mode, &input)
    } else if uri.path().contains("/tasks/decisions/") {
        let id = uri.path().rsplit('/').next().unwrap();
        json!({"call_id":if mode == "mismatched_receipt" {CALL} else {id},"state":"not_sent",
            "replayed":false,"disposition":"unusable","charged_nanos":"0","reserved_nanos":"0"})
    } else {
        assert!(uri.path().ends_with("/activity"));
        json!({"ok":true})
    };
    (StatusCode::OK, axum::Json(reply))
}
