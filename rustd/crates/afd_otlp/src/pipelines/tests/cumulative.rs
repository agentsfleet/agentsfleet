//! Daemon frames carry source-owned totals before any collector routes them.

use afd_observability::metrics::declared::cost::{BILLING_CREDIT_CONSUMED, INVOKE_AGENT_DURATION};
use afd_observability::metrics::registry::{Kind, Temporality};
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{Builder, Duration, Encoding, OTEL_ENDPOINT_KNOB, OtlpConfig, Registry, service};
use crate::pipelines::Exports;

#[derive(Debug)]
struct Receiver {
    endpoint: String,
    frames: mpsc::UnboundedReceiver<Value>,
    server: tokio::task::JoinHandle<()>,
}

async fn accept(
    State(sent): State<mpsc::UnboundedSender<Value>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let _accepted = sent.send(body);
    Json(serde_json::json!({}))
}

async fn receiver() -> Receiver {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("collector socket");
    let endpoint = format!("http://{}", listener.local_addr().expect("bound address"));
    let (sent, frames) = mpsc::unbounded_channel();
    let router = Router::new()
        .route("/v1/metrics", post(accept))
        .with_state(sent);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("collector");
    });
    Receiver {
        endpoint,
        frames,
        server,
    }
}

async fn flush(pipeline: Exports) -> Exports {
    tokio::task::spawn_blocking(move || {
        pipeline.flush();
        pipeline
    })
    .await
    .expect("flush completes")
}

fn metric<'a>(frame: &'a Value, name: &str) -> Option<&'a Value> {
    frame
        .get("resourceMetrics")?
        .as_array()?
        .iter()
        .flat_map(|resource| {
            resource
                .get("scopeMetrics")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|scope| {
            scope
                .get("metrics")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|metric| metric.get("name").and_then(Value::as_str) == Some(name))
}

fn required<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).expect("a required OTLP field")
}

fn integer(value: &Value) -> u64 {
    value.as_u64().unwrap_or_else(|| {
        value
            .as_str()
            .expect("an OTLP integer")
            .parse()
            .expect("a decimal OTLP integer")
    })
}

async fn snapshot(
    received: &mut mpsc::UnboundedReceiver<Value>,
    credits: u64,
    count: u64,
) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let body = received.recv().await.expect("collector stays open");
            let Some(counter) = metric(&body, BILLING_CREDIT_CONSUMED.wire_name()) else {
                continue;
            };
            let Some(histogram) = metric(&body, INVOKE_AGENT_DURATION.wire_name()) else {
                continue;
            };
            assert_eq!(*required(counter, "/sum/aggregationTemporality"), 2);
            assert_eq!(*required(histogram, "/histogram/aggregationTemporality"), 2);
            if integer(required(counter, "/sum/dataPoints/0/asInt")) == credits
                && integer(required(histogram, "/histogram/dataPoints/0/count")) == count
            {
                return body;
            }
        }
    })
    .await
    .expect("the force-flushed frame reaches its collector")
}

fn inspect(body: &Value, sum: f64) -> (Value, Value) {
    let counter = required(
        metric(body, BILLING_CREDIT_CONSUMED.wire_name()).expect("credit metric"),
        "/sum",
    );
    let histogram = required(
        metric(body, INVOKE_AGENT_DURATION.wire_name()).expect("duration metric"),
        "/histogram",
    );
    assert_eq!(*required(counter, "/isMonotonic"), true);
    assert_eq!(*required(histogram, "/dataPoints/0/sum"), sum);
    let starts = (
        required(counter, "/dataPoints/0/startTimeUnixNano").clone(),
        required(histogram, "/dataPoints/0/startTimeUnixNano").clone(),
    );
    assert!(integer(&starts.0) > 0);
    assert!(integer(&starts.1) > 0);
    starts
}

#[test]
fn daemon_families_never_require_collector_accumulation() {
    for family in Registry::declared().expect("daemon census").families() {
        if family.kind != Kind::Gauge {
            assert_eq!(
                family.temporality,
                Some(Temporality::Cumulative),
                "{} must reach any collector with its source-owned total",
                family.name
            );
        }
    }
}

#[tokio::test]
async fn daemon_frames_carry_source_totals_before_any_collector() {
    let mut receiver = receiver().await;
    let config = OtlpConfig::new(&receiver.endpoint, OTEL_ENDPOINT_KNOB)
        .expect("loopback endpoint")
        .with_encoding(Encoding::HttpJson)
        .with_timeout(Duration::from_secs(3));
    let (mut exports, instruments) =
        Builder::new(&config, service(), Registry::declared().expect("census"))
            .install()
            .expect("real pipelines");
    let credit = instruments
        .counter_u64(&BILLING_CREDIT_CONSUMED)
        .expect("credit counter");
    let duration = instruments
        .histogram_f64(&INVOKE_AGENT_DURATION)
        .expect("duration histogram");
    let mut starts = None;
    for (added, elapsed, credits, count, sum) in [
        (2, 0.25, 2, 1, 0.25),
        (3, 0.50, 5, 2, 0.75),
        (4, 0.75, 9, 3, 1.50),
    ] {
        credit.add(added, &[]);
        duration.record(elapsed, &[]);
        exports = flush(exports).await;
        let body = snapshot(&mut receiver.frames, credits, count).await;
        let observed = inspect(&body, sum);
        if let Some(ref previous) = starts {
            assert_eq!(&observed, previous);
        }
        starts = Some(observed);
    }
    tokio::task::spawn_blocking(move || exports.shutdown())
        .await
        .expect("shutdown completes");
    receiver.server.abort();
}
