//! `M233_002` §1 — the daemon's verb spans leave through the installed exporter.
//!
//! A binary of its own: a verb span goes out through the process-wide tracer,
//! and `daemon_suite` holds tests that install their own while others run.
//! Here every test takes [`SERIAL`], so one owner of the global tracer at a
//! time, and the collector is an OTLP/HTTP JSON listener on loopback.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "integration preconditions must fail the test loudly; OTLP JSON is read by key"
)]

use std::time::Duration;

use af_observability::semconv;
use af_observability::verb_span::{SteerStage, VerbClock};
use agentsfleetd::preflight::{OTEL_ENDPOINT_KNOB, OtlpConfig};
use agentsfleetd::supervisor::Supervisor;
use axum::Json;
use axum::extract::{Path, State};
use axum::routing::post;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

#[path = "support/e2e.rs"]
mod e2e;
#[path = "support/e2e_db.rs"]
mod e2e_db;
#[path = "support/e2e_event.rs"]
mod e2e_event;
#[path = "support/e2e_native.rs"]
mod e2e_native;
#[path = "support/e2e_retire.rs"]
mod e2e_retire;
#[path = "support/e2e_seed.rs"]
mod e2e_seed;
#[allow(dead_code, reason = "test support: this binary seeds no tenant keys")]
#[path = "support/e2e_seed_keys.rs"]
mod e2e_seed_keys;
#[path = "support/mod.rs"]
mod support;
#[path = "support/e2e_tail.rs"]
mod tail;
#[path = "support/e2e_wire.rs"]
mod wire;

/// One owner of the process's tracer at a time.
static SERIAL: Mutex<()> = Mutex::const_new(());

/// Past the batch processor's send interval, so a recorded span has left.
const EXPORT_DEADLINE: Duration = Duration::from_secs(20);

/// Two of the gateway's offer probes: long enough for empty passes to run.
const EMPTY_PASSES: Duration = Duration::from_secs(12);

/// Every span name and attribute the collector received.
#[derive(Debug, Clone, Default)]
struct Exported(Vec<(String, Vec<(String, String)>)>);

impl Exported {
    fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a [(String, String)]> {
        self.0
            .iter()
            .filter(move |(span, _)| span == name)
            .map(|(_, attributes)| attributes.as_slice())
    }
}

fn attribute<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

async fn collect(
    Path(signal): Path<String>,
    State(sent): State<mpsc::UnboundedSender<(String, Value)>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let _accepted = sent.send((signal, body));
    Json(serde_json::json!({}))
}

/// A loopback OTLP/HTTP JSON collector and what it has been posted.
async fn collector() -> (String, mpsc::UnboundedReceiver<(String, Value)>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("collector socket");
    let endpoint = format!("http://{}", listener.local_addr().expect("bound address"));
    let (sent, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/v1/{signal}", post(collect))
            .with_state(sent);
        let _served = axum::serve(listener, app).await;
    });
    (endpoint, received)
}

/// OTLP JSON's attribute value, as text.
fn text(value: &Value) -> String {
    ["stringValue", "intValue", "boolValue", "doubleValue"]
        .iter()
        .find_map(|kind| value.get(kind))
        .map(|found| {
            found
                .as_str()
                .map_or_else(|| found.to_string(), str::to_owned)
        })
        .unwrap_or_default()
}

/// Reads every trace delivery so far into `exported`.
fn drain(received: &mut mpsc::UnboundedReceiver<(String, Value)>, exported: &mut Exported) {
    while let Ok((signal, body)) = received.try_recv() {
        if signal != "traces" {
            continue;
        }
        let spans = body["resourceSpans"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|resource| resource["scopeSpans"].as_array().into_iter().flatten())
            .flat_map(|scope| scope["spans"].as_array().into_iter().flatten());
        for span in spans {
            let attributes = span["attributes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|pair| {
                    (
                        pair["key"].as_str().unwrap_or_default().to_owned(),
                        text(&pair["value"]),
                    )
                })
                .collect();
            let name = span["name"].as_str().unwrap_or_default().to_owned();
            exported.0.push((name, attributes));
        }
    }
}

/// Waits until `done` holds over what was exported, or the deadline passes.
async fn exported_until(
    received: &mut mpsc::UnboundedReceiver<(String, Value)>,
    done: impl Fn(&Exported) -> bool,
) -> Exported {
    let mut exported = Exported::default();
    let deadline = tokio::time::Instant::now() + EXPORT_DEADLINE;
    while tokio::time::Instant::now() < deadline {
        drain(received, &mut exported);
        if done(&exported) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    exported
}

/// The daemon's knobs that point its exporter at `endpoint`.
fn exporting_to(endpoint: &str) -> [(&'static str, &str); 2] {
    [
        (OTEL_ENDPOINT_KNOB, endpoint),
        ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json"),
    ]
}

/// One seeded event leased through the gateway and reported.
async fn one_chat(supervisor: &mut Supervisor, endpoint: &str) -> e2e::Scenario {
    let run = e2e::scenario_with(
        supervisor,
        None,
        &exporting_to(endpoint),
        e2e_seed::FLEET_CONFIG_JSON,
    )
    .await;
    let (lease_id, fence) = tail::lease(&run).await;
    let body = wire::report_body(&lease_id, &run.event_id, fence);
    wire::report(&run, &body)
        .await
        .expect("the native report is acknowledged");
    run
}

/// Dimension 1.5: a clock built before `telemetry::install` still reaches the
/// installed exporter, because it asks for the tracer when it records.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "stands up a collector on a real socket: make test-integration-rustd"]
async fn test_verb_span_reaches_exporter_through_global_provider() {
    let _owner = SERIAL.lock().await;
    let (endpoint, mut received) = collector().await;
    let mut clock = VerbClock::<SteerStage>::start();
    clock.mark(SteerStage::Admit);

    let config = OtlpConfig::new(&endpoint, OTEL_ENDPOINT_KNOB)
        .expect("the collector endpoint is an absolute URL")
        .with_encoding(af_otlp::Encoding::HttpJson);
    let (exports, _instruments) =
        agentsfleetd::telemetry::install(&config).expect("the collector builds a transport");
    drop(clock);
    exports.flush();

    let exported = exported_until(&mut received, |seen| {
        seen.named(semconv::SPAN_FLEET_STEER).next().is_some()
    })
    .await;
    let steer = exported
        .named(semconv::SPAN_FLEET_STEER)
        .next()
        .expect("the steer span reached the installed exporter");
    assert!(
        attribute(steer, semconv::ATTR_STEER_ADMIT_MS).is_some(),
        "{steer:?}"
    );
}

/// Dimension 1.3, through the real stack: one chat's offer and report spans
/// carry its event and fleet, each with every stage it reached.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_event_identity_joins_the_verb_spans() {
    let _owner = SERIAL.lock().await;
    let (endpoint, mut received) = collector().await;
    let mut supervisor = Supervisor::new();
    let run = one_chat(&mut supervisor, &endpoint).await;
    let event = run.event_id.to_string();
    let ours = |seen: &Exported, name: &str| {
        seen.named(name)
            .any(|span| attribute(span, semconv::ATTR_EVENT_ID) == Some(event.as_str()))
    };
    let exported = exported_until(&mut received, |seen| {
        ours(seen, semconv::SPAN_FLEET_OFFER) && ours(seen, semconv::SPAN_FLEET_REPORT)
    })
    .await;

    let fleet = run.fleet.to_string();
    for (name, stages) in [
        (
            semconv::SPAN_FLEET_OFFER,
            [
                semconv::ATTR_OFFER_CLAIM_MS,
                semconv::ATTR_OFFER_ADMIT_MS,
                semconv::ATTR_OFFER_ISSUE_MS,
                semconv::ATTR_OFFER_ENCODE_MS,
                semconv::ATTR_OFFER_SEND_MS,
            ]
            .as_slice(),
        ),
        (
            semconv::SPAN_FLEET_REPORT,
            [
                semconv::ATTR_REPORT_LOAD_MS,
                semconv::ATTR_REPORT_PREPARE_MS,
                semconv::ATTR_REPORT_CAPTURE_MS,
                semconv::ATTR_REPORT_COMMIT_MS,
                semconv::ATTR_REPORT_ANNOUNCE_MS,
                semconv::ATTR_REPORT_ACK_MS,
            ]
            .as_slice(),
        ),
    ] {
        let span = exported
            .named(name)
            .find(|span| attribute(span, semconv::ATTR_EVENT_ID) == Some(event.as_str()))
            .unwrap_or_else(|| panic!("no {name} span names event {event}: {exported:?}"));
        assert_eq!(
            attribute(span, semconv::ATTR_AGENT_ID),
            Some(fleet.as_str()),
            "{name}"
        );
        for stage in stages {
            assert!(
                attribute(span, stage).is_some(),
                "{name} lacks {stage}: {span:?}"
            );
        }
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.2: once the one event is reported, the gateway's later offer
/// passes select nothing and record nothing; the only offer span is the claim.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_empty_offer_pass_records_no_span() {
    let _owner = SERIAL.lock().await;
    let (endpoint, mut received) = collector().await;
    let mut supervisor = Supervisor::new();
    let run = one_chat(&mut supervisor, &endpoint).await;
    tokio::time::sleep(EMPTY_PASSES).await;
    let exported = exported_until(&mut received, |seen| {
        seen.named(semconv::SPAN_FLEET_REPORT).next().is_some()
    })
    .await;

    let offers: Vec<_> = exported.named(semconv::SPAN_FLEET_OFFER).collect();
    assert_eq!(offers.len(), 1, "an empty pass recorded a span: {offers:?}");
    supervisor.shutdown().await;
    run.cleanup().await;
}
