//! A daemon and a collector for the binary's own suite, on loopback.
//!
//! The daemon grants one lease, then none, and answers every other runner
//! verb as a healthy daemon would (the replies `afr_supervisor`'s own fake
//! daemon gives). It hands back each report the binary posts. The collector
//! keeps every OTLP/JSON document the binary exports.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test support: a fake that cannot start, or a fixture that will not take a field, is a broken test"
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use afd_wire::paths::{
    LEASE_RENEW_SUFFIX, LEASE_TOOL_CALLS_SUFFIX, RUNNER_HEARTBEATS, RUNNER_LEASES, RUNNER_MEMORY,
    RUNNER_MEMORY_RECALL_SUFFIX, RUNNER_REPORTS, RUNNER_SELF,
};
use axum::http::{Method, header};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// The lease every run is granted, as the supervisor's own suites lease it.
const LEASE_JSON: &str = include_str!("../../afr_supervisor/src/test_support/lease.json");
/// The lease's identifier, which the report echoes.
pub(crate) const LEASE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
/// The fleet the lease runs for.
const FLEET_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// The runner the daemon names this host.
const RUNNER_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8059";
/// A model endpoint under a top-level domain that never resolves (RFC 2606):
/// the turn reaches the real loop and its provider, and fails there without
/// a packet leaving the host.
const UNRESOLVABLE_MODEL: &str = "custom:https://model.invalid/v1";
/// How far ahead the lease and each renewal are granted.
const GRANTED_FOR: Duration = Duration::from_secs(120);
/// The media type every reply carries.
const JSON: &str = "application/json";
/// How often the binary is told to beat and to poll again.
const INTERVAL_MS: u64 = 250;
/// The posture every suite but the per-policy one assigns.
pub(crate) const ALLOW_ALL: &str = "allow_all";

/// The daemon on loopback, and what the binary sent it.
pub(crate) struct FakeDaemon {
    /// The base address the binary is pointed at.
    pub(crate) url: String,
    reports: mpsc::UnboundedReceiver<Value>,
}

impl FakeDaemon {
    /// Serves on a free loopback port, assigning `allow_all`.
    pub(crate) async fn start() -> Self {
        Self::assigning(ALLOW_ALL, &[]).await
    }

    /// Serves on a free loopback port, assigning `network_policy` with
    /// `registry` as its registry baseline.
    pub(crate) async fn assigning(network_policy: &str, registry: &[&str]) -> Self {
        let (sent, reports) = mpsc::unbounded_channel();
        let granted = Arc::new(AtomicBool::new(false));
        let assigned = Arc::new(json!({"sandbox_tier": "landlock_full",
            "network_policy": network_policy, "registry_allowlist": registry,
            "worker_count": 1, "extra_binds": []}));
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let (sent, granted) = (sent.clone(), Arc::clone(&granted));
            let assigned = Arc::clone(&assigned);
            async move {
                let method = request.method().clone();
                let path = request.uri().path().to_owned();
                let body = axum::body::to_bytes(request.into_body(), usize::MAX)
                    .await
                    .unwrap_or_default();
                (
                    [(header::CONTENT_TYPE, JSON)],
                    answer(&method, &path, &body, (&sent, &granted, &assigned)).to_string(),
                )
            }
        });
        let url = serve(app).await;
        Self { url, reports }
    }

    /// The next report the binary posts, or `None` past `within`.
    pub(crate) async fn report(&mut self, within: Duration) -> Option<Value> {
        tokio::time::timeout(within, self.reports.recv())
            .await
            .ok()
            .flatten()
    }
}

/// What the daemon keeps between calls: where reports go, whether the one
/// lease was granted, and the policy it assigns.
type State<'a> = (&'a mpsc::UnboundedSender<Value>, &'a AtomicBool, &'a Value);

/// The daemon's reply to one call.
fn answer(method: &Method, path: &str, body: &[u8], state: State<'_>) -> Value {
    let (reports, granted, assigned) = state;
    let memory = path.starts_with(RUNNER_MEMORY);
    match path {
        RUNNER_HEARTBEATS => json!({
            "status": "ok",
            "assigned_policy": assigned,
            "degraded": false, "degraded_reason": null, "selftest_requested": false,
            "heartbeat_interval_ms": INTERVAL_MS,
        }),
        RUNNER_LEASES if !granted.swap(true, Ordering::SeqCst) => {
            json!({"lease": lease(), "retry_after_ms": null})
        }
        RUNNER_LEASES => json!({"lease": null, "retry_after_ms": INTERVAL_MS}),
        RUNNER_REPORTS => {
            let report = serde_json::from_slice(body).unwrap_or(Value::Null);
            drop(reports.send(report));
            json!({"ok": true})
        }
        RUNNER_SELF => json!({
            "id": RUNNER_ID, "status": "active", "host_id": "host-a",
            "sandbox_tier": "landlock_full", "last_seen_at": 0, "assigned_policy": null,
            "achievable": null, "degraded": false, "degraded_reason": null,
        }),
        _ if path.ends_with(LEASE_RENEW_SUFFIX) => json!({"lease_expires_at": granted_until()}),
        _ if path.ends_with(LEASE_TOOL_CALLS_SUFFIX) => {
            json!({"stored_count": 1, "skipped_count": 0})
        }
        _ if memory && path.ends_with(RUNNER_MEMORY_RECALL_SUFFIX) => {
            json!({"memory": [], "shared": []})
        }
        _ if memory && method == Method::GET => {
            json!({"memory": [], "shared": [], "publish": false})
        }
        _ if memory => json!({"stored": 1, "skipped": 0}),
        _ => json!({"ok": true}),
    }
}

/// The fixture lease, for this suite's fleet, granted from now, naming a
/// model that never resolves.
fn lease() -> Value {
    let mut lease: Value = serde_json::from_str(LEASE_JSON).expect("the fixture lease parses");
    lease["lease_id"] = LEASE_ID.into();
    lease["lease_expires_at"] = granted_until().into();
    lease["event"]["fleet_id"] = FLEET_ID.into();
    lease["policy"]["provider"] = UNRESOLVABLE_MODEL.into();
    lease
}

/// [`GRANTED_FOR`] from now, in Unix milliseconds.
fn granted_until() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past the epoch");
    u64::try_from((now + GRANTED_FOR).as_millis()).expect("milliseconds fit in a u64")
}

/// An OTLP/HTTP receiver standing in for the runner collector.
pub(crate) struct FakeCollector {
    /// The endpoint the binary exports to.
    pub(crate) url: String,
    received: Arc<Mutex<Vec<(String, Value)>>>,
}

impl FakeCollector {
    /// Serves on a free loopback port.
    pub(crate) async fn start() -> Self {
        let received = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&received);
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let kept = Arc::clone(&kept);
            async move {
                let path = request.uri().path().to_owned();
                let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
                    .await
                    .unwrap_or_default();
                let document = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                kept.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((path, document));
                ""
            }
        });
        let url = serve(app).await;
        Self { url, received }
    }

    /// Every document posted to `path`.
    pub(crate) fn posted_to(&self, path: &str) -> Vec<Value> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(at, _)| at == path)
            .map(|(_, document)| document.clone())
            .collect()
    }
}

/// Serves `app` on a free loopback port; its base address.
async fn serve(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let address = listener.local_addr().expect("the bound address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}")
}
