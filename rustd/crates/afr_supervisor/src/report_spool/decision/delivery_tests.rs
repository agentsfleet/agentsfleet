//! The persisted original-owner receipt is retried through actual HTTP, never redispatched.
use super::{DIRECTORY, ReportSpool};
use crate::{
    client::{ControlPlane, HttpRunnerApi},
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN},
    storage_home::StorageHome,
};
use afd_core::id::Uuid7;
use afd_wire::task::{DecisionNotSentCause, DecisionReceipt};
use axum::{
    body::Bytes,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Mutex, mpsc};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const LEASE: &str = "019f0000-0000-7000-8000-000000000001";
const CALL: &str = "019f0000-0000-7000-8000-000000000002";
const OTHER: &str = "019f0000-0000-7000-8000-000000000003";

struct Server {
    plane: ControlPlane,
    seen: mpsc::UnboundedReceiver<Value>,
    replies: Arc<Mutex<VecDeque<(StatusCode, Value)>>>,
    worker: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn start(replies: Vec<(StatusCode, Value)>) -> Result<Self> {
        let replies = Arc::new(Mutex::new(VecDeque::from(replies)));
        let owned = Arc::clone(&replies);
        let (sender, seen) = mpsc::unbounded_channel();
        let app=axum::Router::new().fallback(move |method:Method,uri:Uri,headers:HeaderMap,body:Bytes| {
            let (owned,sender)=(Arc::clone(&owned),sender.clone());
            async move {
                assert_eq!(method,Method::PATCH);
                assert_eq!(uri.path(),format!("/v1/runners/me/leases/{LEASE}/tasks/decisions/{CALL}"));
                assert_eq!(headers.get("authorization").map(axum::http::HeaderValue::as_bytes),Some(b"Bearer agt_r_spool_fixture".as_slice()));
                let input:Value=serde_json::from_slice(&body).unwrap_or_else(|error| unreachable!("native client encoded an invalid receipt: {error}"));
                assert_eq!(input,json!({"fencing_token":17,"receipt":{"outcome":"not_sent","cause":"preflight"}}));
                let _sent=sender.send(input);
                let (status,value)=owned.lock().await.pop_front().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok::<_,StatusCode>((status,axum::Json(value)))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let worker = tokio::spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        let env = afd_core::env::MapEnv::from_pairs([
            (ENV_API_URL, base.as_str()),
            (ENV_RUNNER_TOKEN, "agt_r_spool_fixture"),
        ]);
        let plane = ControlPlane::new(Box::new(HttpRunnerApi::new(&Config::from_env(&env)?)?));
        Ok(Self {
            plane,
            seen,
            replies,
            worker,
        })
    }
    async fn finish(self) {
        assert!(self.replies.lock().await.is_empty());
        self.worker.abort();
    }
}
fn acknowledged(call: &str) -> Value {
    json!({"call_id":call,"state":"not_sent","replayed":true,"disposition":"current","charged_nanos":"0","reserved_nanos":"0"})
}
async fn save(spool: &ReportSpool) -> Result<()> {
    let pending = spool
        .begin_decision(&Uuid7::parse(LEASE)?, &Uuid7::parse(CALL)?, 17)
        .await?;
    let (_pending, saved) = pending
        .finish(DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        })
        .await?;
    saved?;
    Ok(())
}
#[tokio::test]
async fn original_owner_receipt_survives_unauthorized_delivery_then_drains_once() -> Result<()> {
    let root = tempfile::tempdir()?;
    let home = StorageHome::open(root.path())?;
    let spool = ReportSpool::new(&home);
    save(&spool).await?;
    let mut server = Server::start(vec![
        (StatusCode::UNAUTHORIZED, json!({"error_code":"UZ-RUN-001"})),
        (StatusCode::OK, acknowledged(CALL)),
    ])
    .await?;
    let failure = spool
        .drain_decisions(&server.plane)
        .await
        .err()
        .ok_or("unauthorized receipt delivery succeeded")?;
    assert!(failure.is_unauthorized());
    assert!(home.spool().join(DIRECTORY).join(CALL).is_dir());
    assert_eq!(
        server
            .seen
            .recv()
            .await
            .ok_or("missing first attempt")?
            .get("fencing_token"),
        Some(&json!(17))
    );
    // The bounded scan cursor first wraps before revisiting the retained receipt.
    assert!(spool.drain_decisions(&server.plane).await?);
    assert!(spool.drain_decisions(&server.plane).await?);
    assert!(server.seen.recv().await.is_some());
    assert!(!home.spool().join(DIRECTORY).join(CALL).exists());
    assert!(spool.drain_decisions(&server.plane).await?);
    assert!(!spool.drain_decisions(&server.plane).await?);
    server.finish().await;
    Ok(())
}
#[tokio::test]
async fn wrong_acknowledgement_cannot_remove_a_saved_original_owner_receipt() -> Result<()> {
    let root = tempfile::tempdir()?;
    let home = StorageHome::open(root.path())?;
    let spool = ReportSpool::new(&home);
    save(&spool).await?;
    let server = Server::start(vec![(StatusCode::OK, acknowledged(OTHER))]).await?;
    assert!(spool.drain_decisions(&server.plane).await?);
    assert!(home.spool().join(DIRECTORY).join(CALL).is_dir());
    let duplicate = spool
        .begin_decision(&Uuid7::parse(LEASE)?, &Uuid7::parse(CALL)?, 18)
        .await
        .err()
        .ok_or("retained operation was redispatched")?;
    assert_eq!(
        duplicate.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    server.finish().await;
    Ok(())
}
