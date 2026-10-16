//! Telemetry resolves from the OpenTelemetry specification's names alone.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::env::MapEnv;

use super::{OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB, OTEL_TIMEOUT_KNOB, otlp};

/// Where a deployment configured with the standard name sends.
const STANDARD_ENDPOINT: &str = "https://collector.example.test";

/// The header a basic credential is presented in.
const AUTHORIZATION: &str = "Authorization";

/// A credential that is neither real nor secret, as a header value.
const CREDENTIAL: &str = "Basic Zml4dHVyZS1ub3QtYS1jcmVkZW50aWFs";

/// Resolves `pairs`, asserting nothing was faulted.
fn resolved<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> super::OtlpConfig {
    let mut faults = Vec::new();
    let config = otlp(&MapEnv::from_pairs(pairs), &mut faults)
        .expect("an endpoint is configured, so telemetry resolves");
    assert!(faults.is_empty(), "unexpected faults: {faults:?}");
    config
}

/// The header of a given name, if the resolution produced one.
fn header(config: &super::OtlpConfig, name: &str) -> Option<String> {
    config
        .headers()
        .iter()
        .find(|(existing, _value)| existing.eq_ignore_ascii_case(name))
        .map(|(_name, value)| value.clone())
}

/// A deployment that configured nothing exports nothing, and is not at fault.
///
/// The ordinary case — every developer's environment and every test — and the
/// one that must not refuse boot: a daemon that needed a collector to start
/// would make a telemetry backend a prerequisite for running the product.
#[test]
fn no_endpoint_is_no_export_and_no_fault() {
    let mut faults = Vec::new();
    assert!(otlp(&MapEnv::from_pairs([]), &mut faults).is_none());
    assert!(faults.is_empty(), "an absent collector is not a fault");
}

/// The endpoint names the knob it came from, so a boot line can say which.
#[test]
fn the_endpoint_names_its_source() {
    let config = resolved([(OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT)]);
    assert_eq!(config.signal_endpoint(""), STANDARD_ENDPOINT);
    assert_eq!(config.source(), OTEL_ENDPOINT_KNOB);
}

/// Several headers resolve, trimmed, in the order they were written.
#[test]
fn a_header_list_resolves_every_pair() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (
            OTEL_HEADERS_KNOB,
            "x-scope-org-id=tenant-a, x-extra = spaced ",
        ),
    ]);

    assert_eq!(
        header(&config, "x-scope-org-id").as_deref(),
        Some("tenant-a")
    );
    assert_eq!(header(&config, "x-extra").as_deref(), Some("spaced"));
}

/// A timeout is read as milliseconds, exactly as written.
///
/// The unit is the whole risk here. Reading the same digits as seconds turns
/// a one-and-a-half second budget into twenty-five minutes, and every export
/// still succeeds in a test that only checks the knob was accepted.
#[test]
fn a_timeout_is_kept_in_the_milliseconds_it_was_written_in() {
    // pin test: literal is the contract — the knob's unit is milliseconds.
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (OTEL_TIMEOUT_KNOB, "1500"),
    ]);

    assert_eq!(config.timeout(), core::time::Duration::from_millis(1500));
}

/// One malformed pair faults without taking its neighbours with it.
///
/// The whole list is one knob, so the alternative to skipping the bad pair is
/// dropping every header a deployment set — including the credential — over a
/// typo in an unrelated one, and reporting it as a collector that refuses to
/// authenticate.
#[test]
fn a_malformed_pair_faults_without_dropping_its_neighbours() {
    let mut faults = Vec::new();
    let config = otlp(
        &MapEnv::from_pairs([
            (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
            (OTEL_HEADERS_KNOB, "a=1,,no-equals-sign, b=x=y,"),
        ]),
        &mut faults,
    )
    .expect("an endpoint is configured");

    assert_eq!(
        faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
        vec![OTEL_HEADERS_KNOB],
        "the one bad pair faults once, naming the knob it was written in"
    );
    assert_eq!(header(&config, "a").as_deref(), Some("1"));
    assert_eq!(
        header(&config, "b").as_deref(),
        Some("x=y"),
        "a value carrying its own `=` splits at the first one, not at every one"
    );
    assert_eq!(
        config.headers().len(),
        2,
        "an empty pair is skipped rather than sent as a header with no name"
    );
}

/// A name written twice resolves to the last value, once.
///
/// It has to hold for every name: two entries of one header name is a request
/// whose meaning depends on which the client happened to send.
#[test]
fn a_repeated_header_name_keeps_only_the_last_value() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (
            OTEL_HEADERS_KNOB,
            "X-Scope-OrgID=tenant-a,x-scope-orgid=tenant-b",
        ),
    ]);

    assert_eq!(
        header(&config, "x-scope-orgid").as_deref(),
        Some("tenant-b")
    );
    assert_eq!(
        config.headers().len(),
        1,
        "the replaced entry must not survive beside its replacement"
    );
}

/// Every unusable knob is a fault naming itself.
///
/// Refused at boot rather than at the first export: a deployment that asked
/// for gRPC, or wrote a timeout of zero, would otherwise get a daemon that
/// exports nothing and looks exactly like a collector that is down.
#[test]
fn an_unusable_knob_is_a_fault_that_names_itself() {
    for (knob, value) in [
        // pin test: literal is the contract — `grpc` is the spelling the
        // specification defines and this build refuses.
        (OTEL_PROTOCOL_KNOB, "grpc"),
        (OTEL_PROTOCOL_KNOB, "http/xml"),
        (OTEL_TIMEOUT_KNOB, "0"),
        (OTEL_TIMEOUT_KNOB, "soon"),
        (OTEL_HEADERS_KNOB, "no-equals-sign"),
        // Graded here rather than at the exporter, whose own rejection
        // renders the whole endpoint — read from the same place as the
        // credential beside it.
        (OTEL_ENDPOINT_KNOB, "not a url"),
        // A host and port with no scheme parses as a URI, and builds no
        // exporter once a signal path is appended to it.
        (OTEL_ENDPOINT_KNOB, "collector:4318"),
    ] {
        let mut faults = Vec::new();
        // The endpoint case replaces the good one rather than sitting beside
        // it, so a later pair cannot mask the knob under test.
        let pairs = if knob == OTEL_ENDPOINT_KNOB {
            vec![(knob, value)]
        } else {
            vec![(OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT), (knob, value)]
        };
        let _config = otlp(&MapEnv::from_pairs(pairs), &mut faults);
        assert_eq!(
            faults.iter().map(super::Fault::knob).collect::<Vec<_>>(),
            vec![knob],
            "`{knob}={value}` must fault, naming the knob"
        );
    }
}

/// Unset optional knobs resolve to the documented defaults.
#[test]
fn unset_knobs_resolve_to_the_documented_defaults() {
    let config = resolved([(OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT)]);

    assert_eq!(config.encoding(), afd_otlp::Encoding::HttpProtobuf);
    assert_eq!(config.timeout(), afd_otlp::DEFAULT_TIMEOUT);
    assert_eq!(config.headers(), []);
}

/// A resolved configuration never renders the credential it carries.
///
/// The derived `Debug` did: a header can be `Basic <base64>`, and base64 is an
/// encoding rather than a protection. The
/// master key and the session pepper are asserted the same way one module
/// over, and for the same reason — the `{:?}` that ships a token to a log is
/// always one somebody adds later.
#[test]
fn a_resolved_configuration_renders_no_credential() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (OTEL_HEADERS_KNOB, &format!("{AUTHORIZATION}={CREDENTIAL}")),
    ]);

    let rendered = format!("{config:?}");
    assert_eq!(header(&config, AUTHORIZATION).as_deref(), Some(CREDENTIAL));
    assert!(
        !rendered.contains(CREDENTIAL),
        "the credential's bytes must not render: {rendered}"
    );
    assert!(
        rendered.contains(AUTHORIZATION),
        "the header NAME is what a reader needs, and it stays: {rendered}"
    );
}

/// Every bad knob faults in one pass, so one restart fixes all of them: the
/// endpoint is graded last, after every other knob was read.
#[test]
fn every_bad_knob_faults_in_one_pass() {
    let mut faults = Vec::new();

    let config = otlp(
        &MapEnv::from_pairs([
            (OTEL_ENDPOINT_KNOB, "not a url"),
            (OTEL_PROTOCOL_KNOB, "grpc"),
            (OTEL_TIMEOUT_KNOB, "0"),
        ]),
        &mut faults,
    );

    assert!(config.is_none());
    let mut knobs: Vec<&str> = faults.iter().map(super::Fault::knob).collect();
    knobs.sort_unstable();
    let mut expected = vec![OTEL_ENDPOINT_KNOB, OTEL_PROTOCOL_KNOB, OTEL_TIMEOUT_KNOB];
    expected.sort_unstable();
    assert_eq!(knobs, expected);
}
