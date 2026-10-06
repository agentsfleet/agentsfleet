//! The runner's telemetry end to end: the daemon that ships leases one event,
//! the runner's real loop runs it with its export configured, and the lease
//! reaches a collector as one trace of four span kinds that joins the daemon's
//! event by attribute.
//!
//! The runner runs on a thread of its own, on a current-thread runtime, under
//! a subscriber carrying the runner's span layer. The worker pool spawns each
//! lease as a task, and a current-thread runtime polls every task on the
//! thread whose default subscriber that is, so the daemon in the same process
//! keeps exporting nothing through it.
//!
//! The collector is the OTLP/HTTP fixture `integration_telemetry` stands up
//! for the daemon, rather than a collector container: the lane runs its tests
//! inside a container with no route a sidecar could write a result back
//! through, and what this proves is what the runner POSTS.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::sync::PoisonError;

use afd_core::env::MapEnv;
use afd_observability::semconv::{ATTR_EVENT_ID, ATTR_LEASE_ID, SPAN_RUNNER_LEASE};
use afr_telemetry::{Endpoint, Telemetry};
use agentsfleetd::supervisor::Supervisor;
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::bundle_run::{runner, settled};
use crate::e2e::scenario;
use crate::fake_model::{FakeModel, call, say};
use crate::integration_rust_runner::allow_all_egress;
use crate::integration_telemetry::{Received, TRACES, collector};

/// The knobs the runner reads.
const ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const PROTOCOL_KNOB: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";

/// The four span kinds one lease is traced in.
const SPAN_KINDS: [&str; 4] = [SPAN_RUNNER_LEASE, "invoke_agent", "chat", "execute_tool"];

/// The status a lease the runner ran to its end is settled under.
const PROCESSED: &str = "processed";

/// One span a trace export carried: its name, its trace, its string attributes.
#[derive(Debug)]
struct Exported {
    name: String,
    trace: String,
    attributes: Vec<(String, String)>,
}

impl Exported {
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

/// The string under `key`, or empty.
fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Every span the collector's trace posts carried.
fn spans(received: &Received) -> Vec<Exported> {
    let posted = received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    posted
        .iter()
        .filter(|(path, _body)| path == TRACES)
        .filter_map(|(_path, body)| serde_json::from_str::<serde_json::Value>(body).ok())
        .flat_map(|export| array(&export, "resourceSpans"))
        .flat_map(|resource| array(&resource, "scopeSpans"))
        .flat_map(|scope| array(&scope, "spans"))
        .map(|span| Exported {
            name: text(&span, "name"),
            trace: text(&span, "traceId"),
            attributes: array(&span, "attributes")
                .iter()
                .map(|attribute| {
                    let value = attribute
                        .pointer("/value/stringValue")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    (text(attribute, "key"), value.to_owned())
                })
                .collect(),
        })
        .collect()
}

/// A runner pointed at a collector delivers one lease as one trace with four
/// span kinds, its root carrying the lease and the daemon's event.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_e2e_runner_lease_trace_reaches_a_collector() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    // A runner on the default egress policy reads degraded and takes no work.
    allow_all_egress(&run).await;
    let (endpoint, received) = collector().await;
    let env = MapEnv::from_pairs([
        (ENDPOINT_KNOB, endpoint.as_str()),
        (PROTOCOL_KNOB, "http/json"),
    ]);
    let endpoint = Endpoint::from_env(&env)
        .expect("every knob reads")
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the runner's pipeline builds");
    assert!(
        telemetry.recording(),
        "the only install in this test binary is the process recorder"
    );
    let layer = telemetry.layer();
    let (model, _transcript) = FakeModel::new(vec![
        vec![call(
            "call-1",
            "update_plan",
            serde_json::json!({"plan": []}),
        )],
        vec![say("done")],
    ]);
    let home = tempfile::tempdir().expect("a storage home");
    let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
    let shutdown = CancellationToken::new();
    let network = afr_egress::Network::new().expect("the production network builds");
    let serving = runner(
        &run,
        home.path(),
        sandboxes.path(),
        network,
        (model, None),
        shutdown.clone(),
    );
    let thread = std::thread::spawn(move || {
        // The runner's warnings, captured with the test, so a lease that never
        // settles says why.
        let warnings = tracing_subscriber::fmt::layer()
            .with_test_writer()
            .with_filter(LevelFilter::WARN);
        let subscriber = tracing_subscriber::registry().with(layer).with(warnings);
        let _subscriber = tracing::subscriber::set_default(subscriber);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime for the runner")
            .block_on(serving)
    });

    let status = settled(&run, &run.event_id).await;
    shutdown.cancel();
    let stopped = tokio::task::spawn_blocking(move || thread.join())
        .await
        .expect("the join runs")
        .expect("the runner thread does not panic");
    stopped.expect("the runner stops cleanly on shutdown");
    tokio::task::spawn_blocking(move || telemetry.flush())
        .await
        .expect("the flush runs");
    assert_eq!(status.as_deref(), Some(PROCESSED));

    let spans = spans(&received);
    let root = spans
        .iter()
        .find(|span| {
            span.name == SPAN_RUNNER_LEASE && span.attribute(ATTR_EVENT_ID) == Some(&run.event_id)
        })
        .expect("the lease's root span reached the collector, naming the daemon's event");
    assert!(
        root.attribute(ATTR_LEASE_ID)
            .is_some_and(|lease| !lease.is_empty()),
        "the root names its lease: {root:?}"
    );
    let kinds: BTreeSet<&str> = spans
        .iter()
        .filter(|span| span.trace == root.trace)
        .map(|span| span.name.as_str())
        .collect();
    assert_eq!(kinds, BTreeSet::from(SPAN_KINDS), "one trace, four kinds");
    let roots = spans
        .iter()
        .filter(|span| span.trace == root.trace && span.name == SPAN_RUNNER_LEASE)
        .count();
    assert_eq!(roots, 1, "the lease is its own trace");

    supervisor.shutdown().await;
    run.cleanup().await;
}
