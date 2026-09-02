//! Dimension 3.1 — boot builds the transport and supervises its flush.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::env::MapEnv;
use afd_observability::producers::GaugeSources;

use crate::inventory::OTLP_EXPORT;
use crate::preflight::{OTEL_ENDPOINT_KNOB, OTEL_PROTOCOL_KNOB, preflight};
use crate::serve::open_telemetry;
use crate::supervisor::Supervisor;

/// A collector nothing is listening on.
///
/// Deliberately unroutable: the transport is BUILT here, never dialled, and an
/// endpoint that resolved would make this test depend on the network.
const UNREACHABLE: &str = "http://127.0.0.1:1";

/// Everything preflight requires before it will answer at all.
fn required() -> [(&'static str, &'static str); 7] {
    [
        ("DATABASE_URL_API", "postgres://afd:afd@127.0.0.1:5432/agentsfleet"),
        ("REDIS_URL_API", "redis://127.0.0.1:6379"),
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
    pairs.push(("CLERK_SECRET_KEY", "fixture-provider-secret-not-a-credential"));
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

    open_telemetry(&config, &mut supervisor, &GaugeSources::silent())
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

    open_telemetry(&config, &mut supervisor, &GaugeSources::silent())
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

    open_telemetry(&config, &mut supervisor, &GaugeSources::silent())
        .expect("http/json is one of the two encodings this build carries");
    let _report = supervisor.shutdown().await;
}
