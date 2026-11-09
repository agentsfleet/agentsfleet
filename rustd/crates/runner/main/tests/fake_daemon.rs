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

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use af_core::clock::{after, now};
use af_wire::lease::LeaseKind;
use af_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
pub(crate) use afr_supervisor::test_util::{FENCING, LEASE_ID};
use afr_supervisor::test_util::{FLEET_ID, Healthy, LEASE_JSON};
use axum::http::header;
use serde_json::Value;
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
    _server: afr_supervisor::test_util::Listener,
    pub(crate) native: afr_supervisor::test_util::NativeDaemon,
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
        Self::serve(network_policy, registry, Reports::Answered).await
    }

    /// Serves on a free loopback port and keeps each report it is sent, but
    /// never answers one: the lease that posted it stays in flight.
    pub(crate) async fn stalling_reports() -> Self {
        Self::serve(NetworkPolicy::AllowAll, &[], Reports::Stalled).await
    }

    async fn serve(
        network_policy: NetworkPolicy,
        registry: &[&'static str],
        reports_are: Reports,
    ) -> Self {
        let (sent, reports) = mpsc::unbounded_channel();
        let healthy = Arc::new(Healthy {
            assigned: AssignedPolicy {
                durable_tasks_enabled: af_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED,
                decision_model_id: None,
                sandbox_tier: SandboxTier::LandlockFull,
                network_policy,
                registry_allowlist: registry.iter().copied().map(Into::into).collect(),
                worker_count: 1,
                extra_binds: Vec::new(),
            },
            interval_ms: INTERVAL_MS,
            granted_until: jiff::Timestamp::UNIX_EPOCH,
        });
        let native = afr_supervisor::test_util::NativeDaemon::serve(
            Healthy {
                granted_until: granted_until(),
                ..healthy.as_ref().clone()
            },
            lease(),
            move |frame| {
                let sent = sent.clone();
                async move {
                    if frame.get("type").and_then(Value::as_str) == Some("report") {
                        sent.send(
                            frame
                                .get("request")
                                .expect("report carries its request")
                                .clone(),
                        )
                        .expect("the binary test retains its reports");
                        if reports_are == Reports::Stalled {
                            std::future::pending::<()>().await;
                        }
                    }
                }
            },
        )
        .await
        .expect("a native loopback daemon");
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let healthy = Arc::clone(&healthy);
            async move {
                (
                    [(header::CONTENT_TYPE, JSON)],
                    healthy
                        .reply(request.method().as_str(), request.uri().path())
                        .to_string(),
                )
            }
        });
        let server = afr_supervisor::test_util::Listener::serve(app)
            .await
            .expect("a loopback port");
        Self {
            url: server.url.clone(),
            reports,
            _server: server,
            native,
        }
    }

    /// The next report the binary posts, or `None` past `within`.
    pub(crate) async fn report(&mut self, within: Duration) -> Option<Value> {
        tokio::time::timeout(within, self.reports.recv())
            .await
            .ok()
            .flatten()
    }
}

/// Whether the daemon answers the reports it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reports {
    Answered,
    Stalled,
}

/// The fixture lease, tagged as fleet work for this suite's fleet, granted from
/// now, naming a model that never resolves.
fn lease() -> Value {
    let mut lease: Value = serde_json::from_str(LEASE_JSON).expect("the fixture lease parses");
    lease["kind"] = LeaseKind::Fleet.as_str().into();
    lease["lease_id"] = LEASE_ID.into();
    lease["lease_expires_at"] = granted_until().to_string().into();
    lease["event"]["fleet_id"] = FLEET_ID.into();
    lease["policy"]["provider"] = UNRESOLVABLE_MODEL.into();
    lease
}

/// [`GRANTED_FOR`] from now.
fn granted_until() -> jiff::Timestamp {
    after(now(), GRANTED_FOR)
}

/// An OTLP/HTTP receiver standing in for the runner collector.
pub(crate) struct FakeCollector {
    /// The endpoint the binary exports to.
    pub(crate) url: String,
    received: Arc<Mutex<Vec<(String, Value)>>>,
    _server: afr_supervisor::test_util::Listener,
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
        let server = afr_supervisor::test_util::Listener::serve(app)
            .await
            .expect("a loopback port");
        Self {
            url: server.url.clone(),
            received,
            _server: server,
        }
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
