//! A daemon and a collector for the binary's own suite, on loopback.
//!
//! The daemon grants one lease, then none, and answers every other runner
//! verb as a healthy daemon would, through `afr_supervisor`'s own healthy
//! daemon. It hands back each report the binary posts. The collector keeps
//! every OTLP/JSON document the binary exports.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test support: a fake that cannot start, or a fixture that will not take a field, is a broken test"
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use afd_core::clock::{now, saturating_millis_signed};
use afd_wire::paths::{RUNNER_LEASES, RUNNER_REPORTS};
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
pub(crate) use afr_supervisor::test_util::{FENCING, LEASE_ID};
use afr_supervisor::test_util::{FLEET_ID, Healthy, LEASE_JSON};
use axum::http::{Method, header};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// A model endpoint under a top-level domain that never resolves (RFC 2606):
/// the turn reaches the real loop and its provider, and fails there without
/// a packet leaving the host.
const UNRESOLVABLE_MODEL: &str = "custom:https://model.invalid/v1";
/// How far ahead the lease and each renewal are granted.
const GRANTED_FOR: Duration = Duration::from_secs(120);
/// The media type every reply carries.
const JSON: &str = "application/json";
/// How often the binary is told to beat and to poll again.
const INTERVAL_MS: u32 = 250;

/// The daemon on loopback, and what the binary sent it.
pub(crate) struct FakeDaemon {
    /// The base address the binary is pointed at.
    pub(crate) url: String,
    reports: mpsc::UnboundedReceiver<Value>,
}

impl FakeDaemon {
    /// Serves on a free loopback port, assigning `allow_all`.
    pub(crate) async fn start() -> Self {
        Self::assigning(NetworkPolicy::AllowAll, &[]).await
    }

    /// Serves on a free loopback port, assigning `network_policy` with
    /// `registry` as its registry baseline.
    pub(crate) async fn assigning(
        network_policy: NetworkPolicy,
        registry: &[&'static str],
    ) -> Self {
        let (sent, reports) = mpsc::unbounded_channel();
        let granted = Arc::new(AtomicBool::new(false));
        let healthy = Arc::new(Healthy {
            assigned: AssignedPolicy {
                sandbox_tier: SandboxTier::LandlockFull,
                network_policy,
                registry_allowlist: registry.iter().copied().map(Into::into).collect(),
                worker_count: 1,
                extra_binds: Vec::new(),
            },
            interval_ms: INTERVAL_MS,
            granted_until: 0,
        });
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let (sent, granted) = (sent.clone(), Arc::clone(&granted));
            let healthy = Arc::clone(&healthy);
            async move {
                let method = request.method().clone();
                let path = request.uri().path().to_owned();
                let body = axum::body::to_bytes(request.into_body(), usize::MAX)
                    .await
                    .unwrap_or_default();
                (
                    [(header::CONTENT_TYPE, JSON)],
                    answer(&method, &path, &body, (&sent, &granted, &healthy)).to_string(),
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
/// lease was granted, and the healthy daemon it answers as.
type State<'a> = (&'a mpsc::UnboundedSender<Value>, &'a AtomicBool, &'a Healthy);

/// The daemon's reply to one call: the one lease the first time it is asked
/// for, each report kept, and a healthy daemon's reply to everything else,
/// renewals granted from now.
fn answer(method: &Method, path: &str, body: &[u8], state: State<'_>) -> Value {
    let (reports, granted, healthy) = state;
    if path == RUNNER_LEASES && !granted.swap(true, Ordering::SeqCst) {
        return json!({"lease": lease(), "retry_after_ms": null});
    }
    if path == RUNNER_REPORTS {
        drop(reports.send(serde_json::from_slice(body).unwrap_or(Value::Null)));
    }
    let healthy = Healthy {
        granted_until: granted_until(),
        ..healthy.clone()
    };
    healthy.reply(method.as_str(), path)
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
fn granted_until() -> i64 {
    now()
        .saturating_add_millis(saturating_millis_signed(GRANTED_FOR))
        .as_millis()
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
