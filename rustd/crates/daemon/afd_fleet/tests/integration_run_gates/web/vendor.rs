//! A socket vendor which reads the reservation through a separate DB connection.

use af_core::id::Uuid7;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse as _, Response},
    routing::post,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

#[derive(Clone)]
pub(super) struct Reply {
    status: StatusCode,
    body: Value,
    wait: Option<u64>,
    pub(super) delay: Duration,
    pub(super) restrict: bool,
    pub(super) move_fence: bool,
}

impl Reply {
    pub(super) fn new(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            body,
            wait: None,
            delay: Duration::ZERO,
            restrict: false,
            move_fence: false,
        }
    }
    pub(super) fn cooling(mut self, seconds: u64) -> Self {
        self.wait = Some(seconds);
        self
    }
}

#[derive(Clone, Debug)]
pub(super) struct Seen {
    pub(super) name: String,
    pub(super) count: i64,
    pub(super) right_key: bool,
    pub(super) body: Value,
}

#[derive(Clone)]
struct Script {
    db: afd_db::Db,
    run: Uuid7,
    fleet: Uuid7,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

pub(super) struct Vendor {
    server: JoinHandle<()>,
    script: Script,
    base: reqwest::Url,
}

impl Drop for Vendor {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Vendor {
    pub(super) async fn listen(
        db: afd_db::Db,
        run: Uuid7,
        fleet: Uuid7,
        replies: Vec<Reply>,
    ) -> Self {
        let script = Script {
            db,
            run,
            fleet,
            replies: Arc::new(Mutex::new(replies.into())),
            seen: Arc::default(),
        };
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback socket");
        let base = format!("http://{}/", socket.local_addr().expect("loopback address"))
            .parse()
            .expect("vendor URL");
        let app = Router::new()
            .route("/{name}/{operation}", post(receive))
            .with_state(script.clone());
        let server = tokio::spawn(async move {
            axum::serve(socket, app)
                .await
                .expect("serve fake web vendor");
        });
        Self {
            server,
            script,
            base,
        }
    }
    pub(super) fn seen(&self) -> Vec<Seen> {
        self.script
            .seen
            .lock()
            .expect("vendor observations")
            .clone()
    }
    pub(super) fn endpoint(&self, path: &str) -> reqwest::Url {
        self.base.join(path).expect("vendor path")
    }
}

pub(super) fn key(run: &Uuid7, name: &str) -> String {
    format!("{}-{}", run.as_str(), name)
}

async fn receive(
    State(script): State<Script>,
    Path((name, operation)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut connection = script
        .db
        .acquire()
        .await
        .expect("a fresh vendor observation connection");
    let count:Option<i64>=sqlx::query_scalar("SELECT call_count FROM thread.run_web_calls WHERE run_id=$1 AND vendor=$2 AND operation=$3")
        .bind(&script.run).bind(&name).bind(&operation).fetch_optional(&mut *connection).await.expect("observe committed reservation");
    let right_key = headers.get("x-api-key").and_then(|h| h.to_str().ok())
        == Some(key(&script.run, &name).as_str());
    script.seen.lock().expect("observations").push(Seen {
        name,
        count: count.unwrap_or_default(),
        right_key,
        body,
    });
    let reply = script
        .replies
        .lock()
        .expect("scripted replies")
        .pop_front()
        .expect("no unexpected physical dispatch");
    if reply.restrict {
        sqlx::query("UPDATE fleet.fleets SET http_egress=$2 WHERE id=$1")
            .bind(&script.fleet)
            .bind("restricted")
            .execute(&mut *connection)
            .await
            .expect("restrict live fleet");
    }
    if reply.move_fence {
        sqlx::query("UPDATE thread.threads SET fencing_token=fencing_token+1 WHERE fleet_id=$1")
            .bind(&script.fleet)
            .execute(&mut *connection)
            .await
            .expect("fence run before fallback");
    }
    drop(connection);
    tokio::time::sleep(reply.delay).await;
    let mut response = (reply.status, Json(reply.body)).into_response();
    if let Some(wait) = reply.wait {
        response.headers_mut().insert(
            http::header::RETRY_AFTER,
            wait.to_string().parse().expect("delta seconds"),
        );
    }
    response
}
