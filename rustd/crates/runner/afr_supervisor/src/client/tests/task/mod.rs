//! Typed task verbs cross the real runner HTTP transport with fenced identities.
use crate::client::ControlPlane;
use af_core::id::Uuid7;
use af_wire::task::*;
use axum::{
    body::Bytes,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
mod writes;
const ID: &str = "019f0000-0000-7000-8000-000000000001";
const EFFECT: &str = "019f0000-0000-7000-8000-000000000002";
const FENCE: u64 = 31;
struct Request {
    status: StatusCode,
    method: Method,
    path: String,
    body: Value,
    reply: Value,
}
struct Server {
    plane: ControlPlane,
    worker: tokio::task::JoinHandle<()>,
    pending: Arc<Mutex<std::collections::VecDeque<Request>>>,
}
impl Server {
    async fn start(requests: Vec<Request>) -> Self {
        let pending = Arc::new(Mutex::new(std::collections::VecDeque::from(requests)));
        let owned = Arc::clone(&pending);
        let app = axum::Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, bytes: Bytes| {
                let owned = Arc::clone(&owned);
                async move {
                    let expected = owned.lock().unwrap().pop_front().unwrap();
                    assert_eq!(method, expected.method);
                    assert_eq!(uri.to_string(), expected.path);
                    assert_eq!(
                        headers.get("authorization").unwrap(),
                        "Bearer agt_r_task_fixture"
                    );
                    assert_eq!(headers.get("cookie"), None);
                    let body = if bytes.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(&bytes).unwrap()
                    };
                    assert_eq!(body, expected.body);
                    (expected.status, axum::Json(expected.reply))
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let api = super::api(&format!("http://{address}"), "agt_r_task_fixture").unwrap();
        Self {
            plane: ControlPlane::new(Box::new(api)),
            worker,
            pending,
        }
    }
    fn finish(self) {
        assert!(self.pending.lock().unwrap().is_empty());
        self.worker.abort();
    }
}
fn request(method: Method, suffix: &str, body: Value, reply: Value) -> Request {
    Request {
        status: StatusCode::OK,
        method,
        path: format!("/v1/runners/me/leases/{ID}/tasks/{suffix}"),
        body,
        reply,
    }
}
fn effect() -> Value {
    json!({"effect_id":EFFECT,"ordinal":7,"state":"confirmed","expected_head":"a".repeat(40),"tested_tree":"b".repeat(40),"commit":"c".repeat(40),"pull_request_url":"https://github.com/owner/repo/pull/1"})
}
#[tokio::test]
async fn typed_reads_preserve_fences_cursors_and_native_effect_receipts() {
    let inputs = inputs();
    let server = Server::start(read_requests(&inputs)).await;
    read_assertions(&server, &inputs).await;
    server.finish();
}
fn inputs() -> Value {
    json!({"source_admission_id":ID,"accepted_revision_id":ID,"current_message":"private accepted message","inputs":[],"accepted_inputs":[],"current_task":null,"current_interpretation":null,"tasks":[],"next_cursor":"next-input"})
}
fn read_requests(inputs: &Value) -> Vec<Request> {
    vec![
        request(
            Method::GET,
            "inputs?fencing_token=31&limit=4&starting_after=a%2Bb%2F%3D&after_input_sequence=9",
            Value::Null,
            inputs.clone(),
        ),
        request(
            Method::GET,
            &format!(
                "effects?fencing_token=31&task_id={ID}&ordinal_after=6&limit=2&starting_after=c%2Bd%2F%3D"
            ),
            Value::Null,
            json!({"effects":[effect()],"next_cursor":"8"}),
        ),
        request(
            Method::GET,
            &format!("effects/{EFFECT}?fencing_token=31"),
            Value::Null,
            effect(),
        ),
    ]
}
async fn read_assertions(server: &Server, inputs: &Value) {
    let lease = Uuid7::parse(ID).unwrap();
    let read = server
        .plane
        .task_inputs(
            &lease,
            &TaskInputsQuery {
                fencing_token: FENCE,
                starting_after: Some("a+b/=".to_owned()),
                after_input_sequence: Some(9),
                limit: 4,
            },
        )
        .await
        .unwrap();
    assert_eq!(&serde_json::to_value(read).unwrap(), inputs);
    let effects = server
        .plane
        .task_effects(
            &lease,
            &TaskEffectsQuery {
                fencing_token: FENCE,
                task_id: ID.to_owned(),
                ordinal_after: 6,
                starting_after: Some("c+d/=".to_owned()),
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(effects).unwrap(),
        json!({"effects":[effect()],"next_cursor":"8"})
    );
    let read = server
        .plane
        .task_effect(&lease, &Uuid7::parse(EFFECT).unwrap(), FENCE)
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(read).unwrap(), effect());
}
