//! A lease's telemetry: what one lease records, and one lease exported end to
//! end through the runner's own pipeline.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use afd_core::env::MapEnv;
use afd_observability::semconv::{ATTR_EVENT_ID, ATTR_LEASE_ID, SPAN_RUNNER_LEASE};
use afd_wire::activity::StreamTextKind;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;
use afr_agent::{AgentEngine, Loop};
use afr_providers::{Call, Chunk, Connect, End, Provider, Request, Usage};
use afr_sandbox::Limits;
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::testing::{Recorded, Tally, scoped};
use afr_telemetry::{Endpoint, Telemetry};
use afr_tools::Catalog;
use afr_tools::catalog::FILE_READ;
use afr_tools::stub::Stub;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::bundles::BundleCache;
use crate::halt::Halt;
use crate::identity::Whoami;
use crate::lease_loop::Lessee;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, Rig, clock, daemon, lease, plane,
};
use crate::turns::FleetTurns;
use crate::workspace_clone::{GITHUB_ORIGIN, Mirrors};

/// The knobs a runner reads, as the test sets them.
const ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const PROTOCOL_KNOB: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";
/// The signal paths a collector serves.
const TRACES: &str = "/v1/traces";
const METRICS: &str = "/v1/metrics";
/// The four span kinds one lease is traced in.
const SPAN_KINDS: [&str; 4] = [SPAN_RUNNER_LEASE, "invoke_agent", "chat", "execute_tool"];
/// The families one sandboxed lease with one tool call feeds.
const FED: [&str; 3] = [
    "agentsfleet_runner_provider_turn_duration_seconds",
    "agentsfleet_runner_tool_call_duration_seconds",
    "agentsfleet_runner_sandbox_start_duration_seconds",
];

/// A built sandbox is counted ready, and one the host refuses is counted
/// failed, each with how long the attempt took.
#[tokio::test(start_paused = true)]
async fn a_sandbox_start_is_counted_by_how_it_ended() {
    for (refuse, outcome) in [(false, SandboxStart::Ready), (true, SandboxStart::Failed)] {
        let engine = FakeEngine {
            refuse,
            ..FakeEngine::default()
        };
        let rig = Rig::new(
            daemon(|_call| None),
            engine,
            FakeAgent::new(Behaviour::Answer),
        );
        let (tally, recorded) = Tally::new();

        scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID, None)))
            .await
            .unwrap();

        let started: Vec<SandboxStart> = recorded
            .try_iter()
            .filter_map(|recorded| match recorded {
                Recorded::SandboxStart(outcome, _elapsed) => Some(outcome),
                _other => None,
            })
            .collect();
        assert_eq!(started, vec![outcome], "refuse={refuse}");
    }
}

/// A model whose turns are scripted: a call to `file_read`, then an answer.
#[derive(Debug, Clone)]
struct Scripted(Arc<AtomicUsize>);

impl Scripted {
    /// The chunks turn `index` streams.
    fn turn(index: usize) -> Vec<Chunk> {
        let spent = Chunk::Usage(Usage {
            input: 2,
            cached_input: 0,
            cache_written: 0,
            output: 1,
        });
        let said = if index == 0 {
            Chunk::Call(Call {
                id: "call-1".to_owned(),
                name: FILE_READ.name().to_owned(),
                arguments: serde_json::json!({"path": "README.md"}),
            })
        } else {
            Chunk::Text {
                kind: StreamTextKind::Answer,
                text: "done".to_owned(),
            }
        };
        vec![said, spent, Chunk::End(End::default())]
    }
}

impl Connect for Scripted {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _lease: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl Provider for Scripted {
    fn stream<'a>(&'a self, _request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let index = self.0.fetch_add(1, Ordering::SeqCst);
        futures_util::stream::iter(Self::turn(index).into_iter().map(Ok)).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// One request a collector was sent.
#[derive(Debug, Clone)]
struct Posted {
    path: String,
    authorized: bool,
    body: serde_json::Value,
}

/// An OTLP/HTTP receiver standing in for the runner collector, and what it
/// was sent.
async fn collector() -> (String, Arc<Mutex<Vec<Posted>>>) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&received);
    let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
        let kept = Arc::clone(&kept);
        async move {
            let path = request.uri().path().to_owned();
            let authorized = request.headers().contains_key(http_authorization());
            let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
                .await
                .unwrap_or_default();
            let body = serde_json::from_slice(&bytes).unwrap_or_default();
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Posted {
                    path,
                    authorized,
                    body,
                });
            ""
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), received)
}

/// The header a credential would ride in.
fn http_authorization() -> axum::http::HeaderName {
    axum::http::header::AUTHORIZATION
}

/// One span an OTLP/JSON trace export carried.
#[derive(Debug)]
struct Exported {
    name: String,
    trace: String,
    attributes: Vec<(String, String)>,
}

impl Exported {
    /// The span `span` encodes.
    fn of(span: &serde_json::Value) -> Self {
        let text = |key: &str| {
            span.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let attributes = array(span, "attributes")
            .iter()
            .map(|attribute| {
                let key = attribute.get("key").and_then(serde_json::Value::as_str);
                let value = attribute
                    .pointer("/value/stringValue")
                    .and_then(serde_json::Value::as_str);
                (
                    key.unwrap_or_default().to_owned(),
                    value.unwrap_or_default().to_owned(),
                )
            })
            .collect();
        Self {
            name: text("name"),
            trace: text("traceId"),
            attributes,
        }
    }

    /// The string value of attribute `key`.
    fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _value)| name == key)
            .map(|(_name, value)| value.as_str())
    }
}

/// The array under `key`, or none.
fn array(value: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Every span the trace exports carried.
fn spans(posted: &[Posted]) -> Vec<Exported> {
    posted
        .iter()
        .filter(|post| post.path == TRACES)
        .flat_map(|export| array(&export.body, "resourceSpans"))
        .flat_map(|resource| array(&resource, "scopeSpans"))
        .flat_map(|scope| array(&scope, "spans"))
        .map(|span| Exported::of(&span))
        .collect()
}

/// One lease run with the endpoint set delivers its four span kinds under one
/// trace and the runner's families to the collector, and no request carries a
/// credential.
#[tokio::test]
async fn test_runner_exports_spans_and_metrics_when_configured() {
    let (endpoint, received) = collector().await;
    let env = MapEnv::from_pairs([
        (ENDPOINT_KNOB, endpoint.as_str()),
        (PROTOCOL_KNOB, "http/json"),
    ]);
    let endpoint = Endpoint::from_env(&env)
        .unwrap()
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the runner's pipeline builds");
    assert!(
        telemetry.recording(),
        "the only install in this test binary is the process recorder"
    );
    let _global = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    let guard = tracing::subscriber::set_default(subscriber);

    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let (plane, _calls) = plane(daemon(|_call| None));
    let agent: Box<dyn AgentEngine> = Box::new(Loop::new(
        Catalog::new(vec![Stub::boxed(&FILE_READ)]),
        Scripted(Arc::default()),
    ));
    let lessee = Lessee {
        plane,
        engine: Box::new(FakeEngine::default()),
        agent,
        spool: ReportSpool::new(&home),
        bundles: BundleCache::new(&home),
        mirrors: Mirrors::new(home.mirrors(), GITHUB_ORIGIN),
        limits: Limits::default(),
        holds: crate::holds::Holds::start(clock()),
        clock: clock(),
        halt: Halt::new(CancellationToken::new()),
        held: Notify::new(),
        whoami: Whoami::default(),
    };
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let leased = lease(LEASE_ID, FLEET_ID, None);
    lessee.run(&turns, &leased).await.unwrap();
    drop(guard);
    tokio::task::spawn_blocking(move || telemetry.flush())
        .await
        .unwrap();

    let posted = received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert!(
        posted.iter().all(|post| !post.authorized),
        "the runner sent no credential"
    );
    assert!(
        posted.iter().all(|post| post.path != "/v1/logs"),
        "logs stay on stderr"
    );

    let spans = spans(&posted);
    for kind in SPAN_KINDS {
        assert!(
            spans.iter().any(|span| span.name == kind),
            "no `{kind}` span arrived: {spans:?}"
        );
    }
    let traces: std::collections::BTreeSet<&str> =
        spans.iter().map(|span| span.trace.as_str()).collect();
    assert_eq!(traces.len(), 1, "one lease is one trace: {spans:?}");
    let root = spans
        .iter()
        .find(|span| span.name == SPAN_RUNNER_LEASE)
        .expect("the lease span arrived");
    assert_eq!(root.attribute(ATTR_LEASE_ID), Some(LEASE_ID));
    assert_eq!(
        root.attribute(ATTR_EVENT_ID),
        Some(leased.event.event_id.as_ref())
    );

    let metrics: String = posted
        .iter()
        .filter(|post| post.path == METRICS)
        .map(|post| post.body.to_string())
        .collect();
    for family in FED {
        assert!(
            metrics.contains(family),
            "`{family}` never reached the collector"
        );
    }
}
