//! The actual runner client speaks HTTP; native service replies are the boundary fixture.
use super::{
    ID,
    archive::{Archive, Refusal},
    fixture,
};
use crate::{
    client::{ControlPlane, HttpRunnerApi},
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN},
};
use af_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
use axum::{
    body::Bytes,
    http::{HeaderMap, Method, StatusCode, Uri},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub(super) struct Server {
    pub(super) plane: Option<ControlPlane>,
    pub(super) archive: Arc<Mutex<Archive>>,
    worker: tokio::task::JoinHandle<()>,
    _native: crate::test_support::Calls,
}
impl Server {
    pub(super) async fn start(refusal: Refusal) -> Self {
        let archive = Arc::new(Mutex::new(Archive::new(refusal)));
        let owned = Arc::clone(&archive);
        let app = axum::Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, bytes: Bytes| {
                let owned = Arc::clone(&owned);
                async move { respond(&owned, &method, &uri, &headers, &bytes) }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let env = af_core::env::MapEnv::from_pairs([
            (ENV_API_URL, base.as_str()),
            (ENV_RUNNER_TOKEN, "agt_r_recovery_fixture"),
            (crate::config::ENV_STREAM_URL, "https://localhost:4443"),
        ]);
        let api = HttpRunnerApi::new(&Config::from_env(&env).unwrap()).unwrap();
        let native_archive = Arc::clone(&archive);
        let (plane, native) = crate::test_support::plane_with_http(Box::new(api), move |call| {
            let body = call
                .body
                .as_ref()
                .map_or(Value::Null, |bytes| serde_json::from_slice(bytes).unwrap());
            native_archive
                .lock()
                .unwrap()
                .calls
                .push((call.path.to_string(), body));
            crate::test_support::json(&healthy().to(Some(call.verb)))
        });
        Self {
            plane: Some(plane),
            _native: native,
            archive,
            worker,
        }
    }
    pub(super) fn stop(self) {
        self.worker.abort();
    }
}
fn respond(
    archive: &Mutex<Archive>,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    bytes: &Bytes,
) -> (StatusCode, axum::Json<Value>) {
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer agt_r_recovery_fixture"
    );
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes).unwrap()
    };
    let mut archive = archive.lock().unwrap();
    if uri.path().contains("/tasks/") {
        if method == Method::GET {
            assert!(
                uri.query()
                    .unwrap()
                    .contains(&format!("fencing_token={}", super::FENCING))
            );
        } else {
            let expected = if uri.path().ends_with(super::PAGE) {
                Method::PUT
            } else if uri.path().ends_with(super::CHECKPOINT) {
                Method::PATCH
            } else {
                Method::POST
            };
            assert_eq!(method, &expected);
        }
        let (status, reply) = archive.task(uri.path(), body);
        return (status, axum::Json(reply));
    }
    archive.calls.push((uri.path().to_owned(), body));
    (
        StatusCode::OK,
        axum::Json(healthy().reply(method.as_str(), uri.path())),
    )
}
fn healthy() -> crate::test_util::Healthy {
    crate::test_util::Healthy {
        assigned: AssignedPolicy {
            durable_tasks_enabled: true,
            decision_model_id: Some(ID.into()),
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: vec![],
            worker_count: 1,
            extra_binds: vec![],
        },
        interval_ms: 1000,
        granted_until: fixture::deadline(),
    }
}
