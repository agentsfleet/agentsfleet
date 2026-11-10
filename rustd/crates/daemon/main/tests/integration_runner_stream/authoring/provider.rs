//! A real local HTTP provider records physical requests and can hold one response.

use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Semaphore;
use tokio_util::task::AbortOnDropHandle;

use super::{ANSWER, DEADLINE};

pub(super) struct Provider {
    pub(super) state: Arc<Calls>,
    pub(super) endpoint: reqwest::Url,
    owner: AbortOnDropHandle<std::io::Result<()>>,
}

pub(super) struct Calls {
    bodies: Mutex<Vec<Value>>,
    proposal: Mutex<Option<Value>>,
    blocked: AtomicBool,
    entered: Semaphore,
    release: Semaphore,
}

impl Provider {
    pub(super) async fn open(blocked: bool) -> Self {
        let state = Arc::new(Calls {
            blocked: AtomicBool::new(blocked),
            bodies: Mutex::new(Vec::new()),
            proposal: Mutex::new(None),
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("provider bind");
        let endpoint = format!(
            "http://{}/v1/responses",
            listener.local_addr().expect("bound address")
        )
        .parse()
        .expect("local provider URL");
        let app = Router::new()
            .route("/v1/responses", post(respond))
            .with_state(Arc::clone(&state));
        let owner =
            AbortOnDropHandle::new(tokio::spawn(
                async move { axum::serve(listener, app).await },
            ));
        Self {
            state,
            endpoint,
            owner,
        }
    }

    pub(super) async fn close(self) {
        self.owner.abort();
        assert!(
            self.owner
                .await
                .expect_err("provider task aborted")
                .is_cancelled()
        );
    }
}

impl Calls {
    pub(super) fn propose(&self, proposal: Value) {
        *self.proposal.lock().expect("provider proposal") = Some(proposal);
    }

    pub(super) fn count(&self) -> usize {
        self.bodies.lock().expect("provider recording").len()
    }

    pub(super) async fn arrived(&self) {
        tokio::time::timeout(DEADLINE, self.entered.acquire())
            .await
            .expect("physical request deadline")
            .expect("provider entered")
            .forget();
    }

    pub(super) fn resume(&self) {
        self.blocked.store(false, Ordering::SeqCst);
        self.release.add_permits(1);
    }
}

async fn respond(
    State(state): State<Arc<Calls>>,
    Json(body): Json<Value>,
) -> impl axum::response::IntoResponse {
    assert_eq!(
        body.get("stream"),
        Some(&json!(true)),
        "production requests streaming inference"
    );
    state
        .bodies
        .lock()
        .expect("record physical JSON")
        .push(body);
    state.entered.add_permits(1);
    if state.blocked.load(Ordering::SeqCst) {
        state
            .release
            .acquire()
            .await
            .expect("provider release")
            .forget();
    }
    let mut output =
        vec![json!({"type":"message","content":[{"type":"output_text","text":ANSWER}]})];
    let proposal = state.proposal.lock().expect("provider proposal").clone();
    if let Some(proposal) = proposal {
        output.push(
            json!({"type":"function_call","name":"propose_setup","call_id":"edit-request",
            "arguments":json!({"proposal":proposal}).to_string()}),
        );
    }
    let response = json!({"type":"response.completed","response":{
        "status":"completed", "output":output, "usage":{"input_tokens":1,"output_tokens":1}
    }});
    (
        [("content-type", "text/event-stream")],
        format!("data: {response}\n\n"),
    )
}
