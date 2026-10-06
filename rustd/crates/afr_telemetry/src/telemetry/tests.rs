//! The runner's layer exports its own spans and nothing else.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::{Duration, Instant};

use afd_core::env::MapEnv;
use afd_core::test_util::trace;
use afd_observability::semconv::RUNNER_SCOPE_NAME;
use afd_otlp::{OTEL_ENDPOINT_KNOB, OTEL_TIMEOUT_KNOB};
use tracing_subscriber::layer::SubscriberExt as _;

use super::{EVENT_CLOSE_TIMED_OUT, EVENT_DISABLED, EVENT_STARTED, Telemetry, announce_disabled};
use crate::budget::{MAX_LEASE_SPANS, RUNNER_SPANS_PER_SECOND};
use crate::endpoint::Endpoint;

/// The runner's root span, as the supervisor names it.
const LEASE: &str = "runner.lease";

/// A runner child span.
const TOOL: &str = "execute_tool";

/// A pipeline exporting to `endpoint`.
fn installed(pairs: &[(&'static str, &str)]) -> Telemetry {
    let endpoint = Endpoint::from_env(&MapEnv::from_pairs(pairs.iter().copied()))
        .expect("every knob reads")
        .expect("an endpoint is configured");
    Telemetry::install(&endpoint).expect("the pipeline builds")
}

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

/// The production pipeline carries the span budget: a lease far past it
/// hands the exporter no more than one lease's worth.
///
/// Read through the loss count against a collector that refuses everything,
/// so the count is exactly what the sampler let through. One second admits
/// at least its own budget; no lease, however many seconds it spans, passes
/// its per-lease cap.
#[test]
fn the_pipeline_sheds_a_lease_past_its_budget() {
    let endpoint = Endpoint::from_env(&MapEnv::from_pairs([(OTEL_ENDPOINT_KNOB, REFUSING)]))
        .expect("every knob reads")
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the pipeline builds");
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    let calls = MAX_LEASE_SPANS * 2;

    tracing::subscriber::with_default(subscriber, || {
        let lease = tracing::info_span!(target: RUNNER_SCOPE_NAME, "runner.lease");
        let _entered = lease.enter();
        for _call in 0..calls {
            tracing::info_span!(target: RUNNER_SCOPE_NAME, "execute_tool").in_scope(|| {});
        }
    });
    telemetry.flush();

    let sent = telemetry.exports().spans_lost().count();
    // The root is inside the second's budget, so one second admits the root
    // and its budget's remainder.
    let floor = RUNNER_SPANS_PER_SECOND as usize;
    let cap = MAX_LEASE_SPANS as usize;
    assert!(
        (floor..=cap).contains(&sent),
        "{sent} of {} spans reached the exporter; the budget admits {floor} to {cap}",
        calls + 1
    );
}

/// A finished lease gives its slot back through the production pipeline: a
/// runner that has run more leases than its table holds still exports a new
/// lease's children.
///
/// The reaper reads the root's end through the pipeline's one processor, so a
/// bridge or pipeline change that stopped it seeing roots would leak a slot
/// per lease, and once the table filled every later child would be shed. The
/// pause moves the run into a second its roots never charged, so only a
/// leaked slot could shed the last child.
#[test]
fn a_finished_lease_frees_its_slot_through_the_production_pipeline() {
    let telemetry = installed(&[(OTEL_ENDPOINT_KNOB, REFUSING)]);
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    let leases = usize::try_from(afd_core::limits::MAX_WORKERS).unwrap_or(usize::MAX) * 4;

    tracing::subscriber::with_default(subscriber, || {
        for _lease in 0..leases {
            tracing::info_span!(target: RUNNER_SCOPE_NAME, LEASE).in_scope(|| {});
        }
        std::thread::sleep(Duration::from_millis(1100));
        tracing::info_span!(target: RUNNER_SCOPE_NAME, LEASE).in_scope(|| {
            tracing::info_span!(target: RUNNER_SCOPE_NAME, TOOL).in_scope(|| {});
        });
    });
    telemetry.flush();

    assert_eq!(
        telemetry.exports().spans_lost().count(),
        leases + 2,
        "every root, and the last lease's child: the earlier leases gave their slots back"
    );
}

/// Every span the production budget sheds is counted: what reached the
/// exporter and what was counted suppressed add up to every span started.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn every_span_the_budget_sheds_is_counted() {
    use crate::testing::{Recorded, Tally, scoped};

    let telemetry = installed(&[(OTEL_ENDPOINT_KNOB, REFUSING)]);
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    let calls = MAX_LEASE_SPANS * 2;
    let (tally, recorded) = Tally::new();

    scoped(tally, async {
        tracing::subscriber::with_default(subscriber, || {
            let lease = tracing::info_span!(target: RUNNER_SCOPE_NAME, LEASE);
            let _entered = lease.enter();
            for _call in 0..calls {
                tracing::info_span!(target: RUNNER_SCOPE_NAME, TOOL).in_scope(|| {});
            }
        });
    })
    .await;
    let telemetry = tokio::task::spawn_blocking(move || {
        telemetry.flush();
        telemetry
    })
    .await
    .expect("the flush runs");

    let sent = u64::try_from(telemetry.exports().spans_lost().count()).unwrap_or(u64::MAX);
    let shed: u64 = recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            Recorded::SpansSuppressed(spans) => Some(spans),
            _other => None,
        })
        .sum();
    assert_eq!(
        sent + shed,
        u64::from(calls) + 1,
        "every span is either exported or counted as suppressed"
    );
}

/// Closing against a collector that never answers stops waiting at its
/// budget and says so, rather than holding the runner's exit for as long as
/// the export timeout allows.
#[test]
fn closing_against_a_silent_collector_stops_at_its_budget() {
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("a port to never answer on");
    let address = silent.local_addr().expect("the port it took");
    let endpoint = format!("http://{address}");
    let telemetry = installed(&[
        (OTEL_ENDPOINT_KNOB, &endpoint),
        (OTEL_TIMEOUT_KNOB, "60000"),
    ]);
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    tracing::subscriber::with_default(subscriber, || {
        tracing::info_span!(target: RUNNER_SCOPE_NAME, LEASE).in_scope(|| {});
    });

    let capture = trace::Capture::install();
    let started = Instant::now();
    telemetry.close_within(Duration::from_millis(200));

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the close returned at its budget, not the export's 60 s timeout"
    );
    assert_eq!(
        capture.only(EVENT_CLOSE_TIMED_OUT).field("budget_ms"),
        Some("200")
    );
}

/// Closing against a collector that refuses at once finishes inside the
/// budget, with nothing to report.
#[test]
fn closing_against_a_refusing_collector_finishes_quietly() {
    let telemetry = installed(&[(OTEL_ENDPOINT_KNOB, REFUSING)]);

    let capture = trace::Capture::install();
    telemetry.close();

    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_CLOSE_TIMED_OUT))
    );
}

/// The process records into one pipeline's families: a second install in
/// the same process says it is not the recorder.
#[test]
fn a_second_install_is_not_the_recorder() {
    let _first = installed(&[(OTEL_ENDPOINT_KNOB, REFUSING)]);
    let second = installed(&[(OTEL_ENDPOINT_KNOB, REFUSING)]);

    assert!(
        !second.recording(),
        "the first install in this process kept the recorder"
    );
}
