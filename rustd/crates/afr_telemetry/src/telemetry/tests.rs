//! The runner's layer exports its own spans and nothing else.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afd_core::env::MapEnv;
use afd_core::test_util::trace;
use afd_observability::semconv::RUNNER_SCOPE_NAME;
use afd_otlp::OTEL_ENDPOINT_KNOB;
use tracing_subscriber::layer::SubscriberExt as _;

use super::{EVENT_DISABLED, EVENT_STARTED, Telemetry, announce_disabled};
use crate::endpoint::Endpoint;

/// A collector that refuses every connection, promptly: what it loses, the
/// exporter counts, so the count is exactly what reached the exporter.
const REFUSING: &str = "http://127.0.0.1:1";

/// Only the runner's spans reach the exporter: a library's span and a log
/// record inside a runner span are never sent.
///
/// Read through the loss count against a collector that refuses everything,
/// so no server is needed and the number is exactly what was handed over.
#[test]
fn the_layer_exports_the_runners_spans_and_nothing_else() {
    let endpoint = Endpoint::from_env(&MapEnv::from_pairs([(OTEL_ENDPOINT_KNOB, REFUSING)]))
        .expect("every knob reads")
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the pipeline builds");
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());

    tracing::subscriber::with_default(subscriber, || {
        let lease = tracing::info_span!(target: RUNNER_SCOPE_NAME, "runner.lease");
        let _entered = lease.enter();
        tracing::info!(target: RUNNER_SCOPE_NAME, event = "a_log_line", "not a span");
        tracing::info_span!(target: RUNNER_SCOPE_NAME, "execute_tool").in_scope(|| {});
        tracing::info_span!(target: "rig::completions", "a_library_span").in_scope(|| {});
    });
    telemetry.flush();

    assert_eq!(
        telemetry.exports().spans_lost().count(),
        2,
        "the lease and its call reached the exporter; the library span and the \
         log record did not"
    );
}

/// The started line names the knob and the protocol, never the endpoint; the
/// disabled line names the knob that would turn export on.
#[test]
fn the_export_lines_name_the_knob_and_never_the_endpoint() {
    let endpoint = Endpoint::from_env(&MapEnv::from_pairs([(OTEL_ENDPOINT_KNOB, REFUSING)]))
        .expect("every knob reads")
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the pipeline builds");

    let capture = trace::Capture::install();
    telemetry.announce();
    announce_disabled();

    let started = capture.only(EVENT_STARTED);
    assert_eq!(started.field("knob"), Some(OTEL_ENDPOINT_KNOB));
    assert_eq!(started.field("protocol"), Some("http/protobuf"));
    assert_eq!(
        capture.only(EVENT_DISABLED).field("knob"),
        Some(OTEL_ENDPOINT_KNOB)
    );
    let rendered = format!("{:?}", capture.events());
    assert!(
        !rendered.contains(REFUSING),
        "the endpoint's value never reaches a log: {rendered}"
    );
}
