//! Boot builds the transport and supervises its flush, from the same knobs the
//! moved builder reads.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::env::MapEnv;
use afd_observability::producers::GaugeSources;

use afd_otlp::Encoding;

use crate::inventory::OTLP_EXPORT;
use crate::preflight::{
    GRAFANA_API_KEY_KNOB, GRAFANA_INSTANCE_KNOB, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB,
    OTEL_PROTOCOL_KNOB, OTEL_TIMEOUT_KNOB, preflight,
};
use crate::serve::{attach_exports, open_telemetry};
use crate::supervisor::Supervisor;

/// A collector nothing is listening on.
///
/// Deliberately unroutable: the transport is BUILT here, never dialled, and an
/// endpoint that resolved would make this test depend on the network.
const UNREACHABLE: &str = "http://127.0.0.1:1";

/// Everything preflight requires before it will answer at all.
fn required() -> [(&'static str, &'static str); 7] {
    [
        (
            "DATABASE_URL_API",
            "postgres://afd:afd@127.0.0.1:5432/agentsfleet",
        ),
        ("DRAGONFLY_URL", "redis://127.0.0.1:6379"),
        (
            "ENCRYPTION_MASTER_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ),
        (
            "AUTH_SESSION_CODE_PEPPER",
            "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        ),
        ("OIDC_ISSUER", "https://identity.fixture.test"),
        ("OIDC_AUDIENCE", "agentsfleetd-lane"),
        ("CLERK_API_BASE", "https://api.identity.fixture.test"),
    ]
}

/// A resolved configuration carrying `extra` beside the required knobs.
fn configured(extra: &[(&str, &str)]) -> crate::preflight::BootConfig {
    let mut pairs: Vec<(&str, &str)> = required().to_vec();
    pairs.push((
        "CLERK_SECRET_KEY",
        "fixture-provider-secret-not-a-credential",
    ));
    pairs.extend(extra.iter().copied());
    preflight(&MapEnv::from_pairs(pairs)).expect("the fixture environment resolves")
}

/// With an endpoint configured, boot supervises the flush under the name the
/// inventory declares — and that task joins when it is cancelled.
///
/// Both halves matter and they fail differently. A transport built and never
/// supervised exports whatever the SDK's own timers manage and loses the rest
/// at shutdown, silently. A task that will not stop when cancelled holds the
/// process open past its drain deadline, and the supervisor reports it by name
/// rather than hanging — which is what the join assertion below reads.
#[tokio::test]
async fn boot_supervises_the_export_under_its_inventoried_name() {
    let config = configured(&[(OTEL_ENDPOINT_KNOB, UNREACHABLE)]);
    let mut supervisor = Supervisor::new();

    // Installed FIRST so `open_telemetry` finds a slot to attach to. Without
    // it the attach branch is skipped and boot exports through a subscriber
    // that never carries the bridges — the configuration a deployment never
    // runs, and the one this test would otherwise be proving. Whether this
    // call or an earlier test won the process-wide slot does not matter: the
    // slot is set either way by the time the assertion below reads it.
    let _installed = crate::logs::install(&MapEnv::default());

    open_telemetry(
        attach_exports(&config).expect("the endpoint builds a transport"),
        &mut supervisor,
        &GaugeSources::silent(),
    )
    .expect("an endpoint the exporter can parse builds a transport");

    assert_eq!(
        supervisor.inventory(),
        vec![OTLP_EXPORT],
        "the flush loop is supervised, and under the name the inventory declares"
    );

    let report = supervisor.shutdown().await;
    assert_eq!(report.joined, vec![OTLP_EXPORT]);
    assert!(
        report.is_clean(),
        "the export task must stop when it is cancelled: {report:?}"
    );
}

/// With no endpoint, boot supervises nothing and still succeeds.
///
/// The ordinary case — every developer's environment and most tests — and the
/// reason `integration_serve.rs` asserts an inventory without this task in it.
#[tokio::test]
async fn no_endpoint_supervises_nothing_and_is_not_a_failure() {
    let config = configured(&[]);
    let mut supervisor = Supervisor::new();

    open_telemetry(
        attach_exports(&config).expect("the endpoint builds a transport"),
        &mut supervisor,
        &GaugeSources::silent(),
    )
    .expect("a deployment that exports nothing still boots");

    assert!(
        supervisor.inventory().is_empty(),
        "nothing to flush means nothing to supervise"
    );
}

/// The JSON encoding builds a transport too.
///
/// The knob accepts two values, so both have to reach an exporter — a build
/// that only ever succeeded for the default would leave the other spelling
/// accepted at preflight and broken at boot.
#[tokio::test]
async fn the_json_protocol_builds_a_transport() {
    let config = configured(&[
        (OTEL_ENDPOINT_KNOB, UNREACHABLE),
        (OTEL_PROTOCOL_KNOB, "http/json"),
    ]);
    let mut supervisor = Supervisor::new();

    open_telemetry(
        attach_exports(&config).expect("the endpoint builds a transport"),
        &mut supervisor,
        &GaugeSources::silent(),
    )
    .expect("http/json is one of the two encodings this build carries");
    let _report = supervisor.shutdown().await;
}

/// The daemon builds the same pipelines from the same knobs after the move.
///
/// The builder left this crate for `afd_otlp`, and the risk of a move is a
/// knob that stops reaching the exporter on the way: a vendor credential that
/// no longer becomes a header, a JSON request that posts protobuf, a timeout
/// read in the wrong unit. So every knob the daemon reads is set at once, and
/// what reaches the transport is read back — endpoint per signal, encoding,
/// timeout, header names — then the transport is built, log pipeline
/// included, because the daemon is the binary that bridges its records.
#[tokio::test]
async fn test_daemon_otlp_install_is_unchanged() {
    let config = configured(&[
        (OTEL_ENDPOINT_KNOB, "http://otelcol-dev.internal:4318/"),
        (OTEL_PROTOCOL_KNOB, "http/json"),
        (OTEL_TIMEOUT_KNOB, "1500"),
        (OTEL_HEADERS_KNOB, "x-scope-orgid=tenant-a"),
        (GRAFANA_INSTANCE_KNOB, "123456"),
        (GRAFANA_API_KEY_KNOB, "fixture-token-not-a-credential"),
    ]);
    let otlp = config.otlp().expect("an endpoint is configured");

    for (path, expected) in [
        ("/v1/traces", "http://otelcol-dev.internal:4318/v1/traces"),
        ("/v1/metrics", "http://otelcol-dev.internal:4318/v1/metrics"),
        ("/v1/logs", "http://otelcol-dev.internal:4318/v1/logs"),
    ] {
        assert_eq!(otlp.signal_endpoint(path), expected);
    }
    assert_eq!(otlp.encoding(), Encoding::HttpJson);
    assert_eq!(otlp.timeout(), std::time::Duration::from_millis(1500));
    let names: Vec<&str> = otlp
        .headers()
        .iter()
        .map(|(name, _value)| name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Authorization", "x-scope-orgid"],
        "the vendor pair still becomes a credential header beside the standard ones"
    );
    assert_eq!(otlp.source(), OTEL_ENDPOINT_KNOB);

    let (exports, _instruments) = super::install(otlp).expect("the knobs build every pipeline");
    assert!(
        exports.logger().is_some(),
        "the daemon still bridges its log records, so it still builds a log pipeline"
    );
    tokio::task::spawn_blocking(move || exports.flush())
        .await
        .expect("the flush runs to completion");
}

/// The resident reading is a real measurement or nothing, never a guess.
///
/// Linux answers from `/proc/self/statm`; every other host has no such file and
/// gets `None`. That asymmetry is the design — a number invented for a
/// developer's macOS box would be a measurement nobody took, reported as one
/// that was — so the assertion is conditional on the platform rather than on
/// the value.
///
/// The positive arm matters more than it looks: `statm` answers in PAGES, and a
/// reading that forgot to multiply would be off by four thousand and still look
/// like a plausible byte count.
#[test]
fn the_resident_reading_is_a_measurement_or_nothing() {
    let reading = super::resident_bytes();

    if cfg!(target_os = "linux") {
        let bytes = reading.unwrap_or_default();
        assert!(
            bytes > 0,
            "a running process holds a resident set; zero means the pages were \
             read but not converted"
        );
    } else {
        assert!(
            reading.is_none(),
            "a host with no /proc reports nothing rather than a fabricated size"
        );
    }
}

/// A freshly built pipeline has lost nothing, on any of the three signals.
///
/// These counters are what separates "exporting fine" from "exporting into a
/// void": they are the numbers `agentsfleet_otlp_entries_discarded_total` is
/// built from, and nothing else in the daemon reads them. A getter wired to
/// the wrong field would therefore be caught by nothing — while reporting a
/// lossy process as healthy, which is the direction that costs an incident.
///
/// Three signals rather than one because they count different things: spans
/// and records are discrete items, metric cycles are moments.
#[tokio::test]
async fn a_new_pipeline_reports_no_losses_on_any_signal() {
    let config = configured(&[(OTEL_ENDPOINT_KNOB, UNREACHABLE)]);
    let otlp = config.otlp().expect("an endpoint is configured");

    let (exports, _instruments) =
        super::install(otlp).expect("a well-formed endpoint builds every pipeline");

    assert_eq!(
        exports.spans_lost().count(),
        0,
        "a pipeline that has exported nothing has dropped no spans"
    );
    assert_eq!(exports.records_lost().count(), 0, "nor any log records");
    assert_eq!(
        exports.cycles_lost().failed(),
        0,
        "nor any metric collection cycles"
    );
}
