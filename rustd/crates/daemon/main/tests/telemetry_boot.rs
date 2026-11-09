//! Real boot sampling and failure flush with an exclusively owned subscriber.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions must fail the test loudly"
)]

use std::time::Duration;

use af_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use agentsfleetd::{BootFailure, Supervisor};
use axum::Json;
use axum::extract::{Path, State};
use axum::routing::post;
use serde_json::Value;
use tokio::sync::mpsc;

mod support;

type Delivery = (String, Value);

async fn collect(
    Path(signal): Path<String>,
    State(sent): State<mpsc::UnboundedSender<Delivery>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let _accepted = sent.send((signal, body));
    Json(serde_json::json!({}))
}

fn environment(database: &str, queue: &str, endpoint: &str) -> MapEnv {
    let pairs: Vec<_> = [
        ("DATABASE_URL_API", database.to_owned()),
        ("DRAGONFLY_URL", queue.to_owned()),
        (
            "NATS_URL",
            ProcessEnv.get("TEST_NATS_URL").expect("lane bus seeds"),
        ),
        (
            "DRAGONFLY_TLS_CA_CERT_FILE",
            ProcessEnv
                .get("TEST_DRAGONFLY_CA_CERT")
                .expect("lane queue trust"),
        ),
        (
            "ENCRYPTION_MASTER_KEY",
            support::SESSION_PEPPER
                .first()
                .expect("fixture pepper")
                .1
                .to_owned(),
        ),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint.to_owned()),
        ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json".to_owned()),
        ("OTEL_EXPORTER_OTLP_TIMEOUT", "1000".to_owned()),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", "1250".to_owned()),
    ]
    .into_iter()
    .chain(
        support::SESSION_PEPPER
            .into_iter()
            .chain(support::IDENTITY)
            .map(|(name, value)| (name, value.to_owned())),
    )
    .collect();
    MapEnv::from_pairs(pairs.iter().map(|(name, value)| (*name, value.as_str())))
}

fn metrics(body: &Value) -> impl Iterator<Item = &Value> {
    body["resourceMetrics"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| resource["scopeMetrics"].as_array().into_iter().flatten())
        .flat_map(|scope| scope["metrics"].as_array().into_iter().flatten())
}

fn has_event(body: &Value, event: &str) -> bool {
    body["resourceLogs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| resource["scopeLogs"].as_array().into_iter().flatten())
        .flat_map(|scope| scope["logRecords"].as_array().into_iter().flatten())
        .any(|record| {
            record["eventName"] == event
                || record["attributes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|pair| {
                        pair["key"] == "event"
                            && pair.pointer("/value/stringValue").and_then(Value::as_str)
                                == Some(event)
                    })
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs real datastores and a collector socket: make test-integration-rustd"]
async fn boot_exports_live_snapshots_and_flushes_before_returning_a_dependency_failure() {
    assert!(agentsfleetd::logs::install(&MapEnv::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("collector socket");
    let endpoint = format!("http://{}", listener.local_addr().expect("bound address"));
    let (sent, mut received) = mpsc::unbounded_channel();
    let collector = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/v1/{signal}", post(collect))
                .with_state(sent),
        )
        .await
        .expect("collector serves until aborted");
    });
    let database = ProcessEnv.get("TEST_DATABASE_URL").expect("lane database");
    let queue = ProcessEnv.get("TEST_DRAGONFLY_URL").expect("lane queue");
    let mut supervisor = Supervisor::new();
    let booted = agentsfleetd::serve::boot(
        &environment(&database, &queue, &endpoint),
        0,
        &mut supervisor,
    )
    .await
    .expect("real daemon boot");

    tokio::time::timeout(Duration::from_secs(15), async {
        // Only Linux has `/proc/self/statm`; every other host reports no
        // resident reading by design (`the_resident_reading_is_a_measurement_or_nothing`),
        // so the wait asks for one only where one can exist.
        let mut resident = !cfg!(target_os = "linux");
        let mut catalogue = false;
        let mut schedules = false;
        while !(resident && catalogue && schedules) {
            let (signal, body) = received.recv().await.expect("collector remains live");
            if signal == "metrics" {
                for metric in metrics(&body) {
                    if metric["name"] == "agentsfleet_process_resident_memory_bytes" {
                        resident |= metric
                            .pointer("/gauge/dataPoints")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .any(|point| {
                                point["asInt"]
                                    .as_str()
                                    .and_then(|value| value.parse::<u64>().ok())
                                    .is_some_and(|value| value > 0)
                            });
                    }
                    catalogue |= metric["name"] == "agentsfleet_model_catalog_snapshot_age_seconds";
                    schedules |= metric["name"] == "agentsfleet_schedules";
                }
            }
        }
    })
    .await
    .expect("sampler publishes real memory, catalogue and schedule readings");
    let report = supervisor.shutdown().await;
    assert!(report.is_clean(), "{report:?}");
    drop(booted);
    while received.try_recv().is_ok() {}

    let mut failed = Supervisor::new();
    let failure = agentsfleetd::serve::boot(
        &environment(&database, "redis://127.0.0.1:1", &endpoint),
        0,
        &mut failed,
    )
    .await
    .expect_err("an unreachable queue refuses boot after the database opened");
    assert!(matches!(failure, BootFailure::Queue(_)));
    assert_eq!(
        failure.code(),
        af_core::error_code::STARTUP_DRAGONFLY_CONNECT
    );
    assert_eq!(failed.inventory(), Vec::<&str>::new());
    // The flush has finished when boot returns; do not await another export.
    let mut initialized = false;
    while let Ok((signal, body)) = received.try_recv() {
        initialized |= signal == "logs" && has_event(&body, "pool_initialized");
    }
    assert!(
        initialized,
        "failed boot must deliver its buffered pool record"
    );
    collector.abort();
}
