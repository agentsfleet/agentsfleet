//! §4 — the failure posture, and all three signals actually leaving.
//!
//! # Why a collector fixture rather than a mocked exporter
//!
//! Everything up to the exporter is already covered by unit tests that stub
//! one. What those cannot answer is whether the pipeline this daemon BUILDS —
//! resource, encoder, client, endpoint path — produces something a collector
//! receives, and that is the only question the cutover actually rests on. So
//! the fixture is an HTTP server, the daemon posts to it, and the assertions
//! are about what arrived.
//!
//! The fixture speaks `http/json` for the same reason: a protobuf body would
//! have to be decoded to be asserted on, which would mean testing this
//! daemon's export through a second implementation of the same encoding.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use afd_observability::producers::GaugeSources;
use afd_observability::semconv;
use agentsfleetd::preflight::{OTEL_ENDPOINT_KNOB, OtlpConfig};
use agentsfleetd::telemetry::{Exports, install};
use axum::Router;
use axum::extract::State;
use axum::routing::post;
use opentelemetry::logs::{LogRecord as _, Logger as _, LoggerProvider as _};
use opentelemetry::trace::{Tracer as _, TracerProvider as _};

/// What the collector saw: the path, and the body it was sent.
type Received = Arc<Mutex<Vec<(String, String)>>>;

/// A signal path the fixture accepts.
const TRACES: &str = "/v1/traces";
const METRICS: &str = "/v1/metrics";
const LOGS: &str = "/v1/logs";

/// An event name the Zig daemon emits, and this one must keep.
///
/// Chosen because it is a boundary pair's half and a dashboard matches on it:
/// the port rule is that a Rust replacement keeps the Zig spelling, and the
/// only way to grade that is to read what left the process.
const PORTED_EVENT: &str = "supervised_task_started";

/// How long a flush is given before the assertions read what arrived.
const DELIVERY_GRACE: Duration = Duration::from_millis(500);

/// A collector that keeps what it is posted.
async fn collector() -> (String, Received) {
    let received: Received = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(TRACES, post(accept))
        .route(METRICS, post(accept))
        .route(LOGS, post(accept))
        .with_state(Arc::clone(&received));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port is available");
    let address = listener.local_addr().expect("the port is bound");
    tokio::spawn(async move {
        let _served = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), received)
}

/// Records one delivery and accepts it.
async fn accept(State(received): State<Received>, request: axum::extract::Request) -> &'static str {
    let path = request.uri().path().to_owned();
    let body = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((path, String::from_utf8_lossy(&body).into_owned()));
    ""
}

/// The configuration a test points at `endpoint`.
fn configured(endpoint: &str) -> OtlpConfig {
    OtlpConfig {
        endpoint: endpoint.into(),
        source: OTEL_ENDPOINT_KNOB,
        headers: Vec::new(),
        // See the module note: JSON so the assertions read the body rather
        // than decoding it through a second protobuf implementation.
        protocol: "http/json".into(),
        timeout: Duration::from_secs(2),
    }
}

/// Emits one of each signal through `exports`.
fn emit_every_signal(exports: &Exports) {
    let span = exports
        .tracer()
        .tracer(semconv::SCOPE_NAME)
        .start("unit-of-work");
    drop(span);

    let logger = exports.logger().logger(semconv::SCOPE_NAME);
    let mut record = logger.create_log_record();
    record.set_event_name(PORTED_EVENT);
    record.set_body(PORTED_EVENT.into());
    logger.emit(record);

    opentelemetry::global::meter(semconv::SCOPE_NAME)
        .u64_counter("units_of_work")
        .build()
        .add(1, &[]);
}

/// Every path the collector was posted to.
fn paths(received: &Received) -> Vec<String> {
    received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(path, _body)| path.clone())
        .collect()
}

/// Every body the collector was posted, joined.
fn bodies(received: &Received) -> String {
    received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(_path, body)| body.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Dimension 4.2 — all three signals reach a collector, carrying this
/// process's identity and the event names the daemon this ports emits.
///
/// The log half is the one worth having. A transport that carried metrics and
/// spans but not logs would take the log backend dark at the swap with nothing
/// to catch it — the signal nobody checks is the one that disappears quietly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "stands up a collector on a real socket: make test-integration-rustd"]
async fn all_three_signals_reach_a_collector() {
    let (endpoint, received) = collector().await;
    let exports = install(&configured(&endpoint), &GaugeSources::silent())
        .expect("the fixture endpoint builds a transport");

    emit_every_signal(&exports);
    exports.flush();
    tokio::time::sleep(DELIVERY_GRACE).await;

    let delivered = paths(&received);
    for signal in [TRACES, METRICS, LOGS] {
        assert!(
            delivered.iter().any(|path| path == signal),
            "nothing arrived at {signal}; the collector saw {delivered:?}"
        );
    }

    let bodies = bodies(&received);
    assert!(
        bodies.contains(PORTED_EVENT),
        "the log record must carry the event name the Zig daemon emits, so a \
         dashboard matching on it keeps matching across the swap"
    );
    assert!(
        bodies.contains(semconv::SCOPE_NAME),
        "every signal carries this service's identity, or nothing downstream \
         can correlate the three"
    );
}

/// Dimension 4.1 — an unreachable collector costs telemetry, never latency.
///
/// Measured against the emit rather than against a wall-clock budget: the
/// claim is that recording does not WAIT for the export, and a fixed budget
/// would fail spuriously on a loaded machine while proving nothing extra.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "opens a socket to an unroutable port: make test-integration-rustd"]
async fn an_unreachable_collector_costs_spans_and_not_requests() {
    // Port 1 on the loopback: nothing listens, and the connection is refused
    // rather than left hanging, so the export fails promptly and definitely.
    let exports = install(&configured("http://127.0.0.1:1"), &GaugeSources::silent())
        .expect("an unreachable endpoint still BUILDS — nothing is dialled here");

    assert_eq!(exports.spans_lost().count(), 0, "nothing has failed yet");

    let started = Instant::now();
    emit_every_signal(&exports);
    let emitting = started.elapsed();

    exports.flush();
    tokio::time::sleep(DELIVERY_GRACE).await;

    assert!(
        emitting < DELIVERY_GRACE,
        "emitting took {emitting:?} — the export is on the caller's path"
    );
    assert!(
        exports.spans_lost().count() > 0,
        "a refused collector must COUNT what it lost; silence would be a \
         process that discards telemetry while being trusted"
    );
}

/// The stderr subscriber's reload slot takes the export bridges.
///
/// `logs.rs` was the worst-covered file in the daemon, and the reason is
/// structural rather than neglectful: `attach` needs a real `Exports` to build
/// its two bridges from, and no unit test has one. This suite does — it builds
/// a live pipeline against a collector fixture two tests up — so the seam gets
/// covered where the fixture already is.
///
/// What is being held:
///
/// - `install` answers TRUE for the caller that took the process-wide slot and
///   FALSE for anyone after, which is the answer boot reads to decide whether
///   it owns the subscriber.
/// - `attach` fills the reload slot with the span and record bridges over the
///   SAME emits stderr already writes, and answers whether the swap took.
/// - `Signals` renders without leaking its handle's innards, because a `{:?}`
///   somebody adds later must not print a subscriber's guts into a log line.
///
/// The last two are OPPORTUNISTIC here and must not be read as this suite's
/// proof of them. The process-wide slot is won by whoever installs a subscriber
/// first, and in this binary that is usually `support`, so `signals()` answers
/// `None` and the assertions below are skipped — silently, and depending on
/// test order. `logs::tests::the_reload_slot_takes_the_export_bridges` proves
/// both against a handle of its own, deterministically and with no collector.
/// What is left here is the one claim that needs a REAL pipeline: that a
/// `Signals` taken from the live process accepts bridges built from an
/// `Exports` this suite actually exported through.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "stands up a collector on a real socket: make test-integration-rustd"]
async fn the_subscriber_slot_takes_the_export_bridges() {
    let (endpoint, _received) = collector().await;
    let exports = install(&configured(&endpoint), &GaugeSources::silent())
        .expect("the pipeline builds against the fixture");

    // First installer wins the process-wide slot; every later one is told so
    // rather than silently replacing a subscriber somebody is already reading.
    let took = agentsfleetd::logs::install(&afd_core::env::MapEnv::default());
    let took_again = agentsfleetd::logs::install(&afd_core::env::MapEnv::default());
    assert!(
        !took_again,
        "a second install answers false — the global default is set once"
    );

    let Some(signals) = agentsfleetd::logs::signals() else {
        assert!(
            !took,
            "install answered true, so the slot must hold the handle it set"
        );
        return;
    };

    assert!(
        !format!("{signals:?}").contains("Handle"),
        "the debug rendering names the type without unfolding the subscriber \
         handle it wraps"
    );
    assert!(
        signals.attach(&exports),
        "the reload slot accepts the span and record bridges"
    );
}
