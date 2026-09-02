//! Dimension 3.2 — the standard names configure, the vendor names bridge.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::env::MapEnv;

use super::{
    AUTHORIZATION, GRAFANA_API_KEY_KNOB, GRAFANA_ENDPOINT_KNOB, GRAFANA_INSTANCE_KNOB,
    OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB, OTEL_TIMEOUT_KNOB, otlp,
};

/// Where a deployment configured with the standard name sends.
const STANDARD_ENDPOINT: &str = "https://collector.example.test";

/// Where one still carrying the vendor name sends.
const VENDOR_ENDPOINT: &str = "https://otlp.vendor.example.test";

/// A vendor account, and a token that is neither real nor secret.
const VENDOR_INSTANCE: &str = "123456";
const VENDOR_KEY: &str = "fixture-token-not-a-credential";

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
        .headers
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

/// The vendor spelling alone still exports, which is what a rollback needs.
#[test]
fn the_vendor_endpoint_alone_still_exports() {
    let config = resolved([(GRAFANA_ENDPOINT_KNOB, VENDOR_ENDPOINT)]);

    assert_eq!(&*config.endpoint, VENDOR_ENDPOINT);
    assert_eq!(
        config.source, GRAFANA_ENDPOINT_KNOB,
        "the source names the knob it came from, so a boot line can say which"
    );
}

/// With both set, the standard name wins.
///
/// The property that makes it an ALIAS. If the vendor spelling could outrank
/// the name it stands in for, a deployment that had adopted the standard one
/// would keep exporting to wherever the leftover variable pointed — silently,
/// and for as long as nobody compared the two.
#[test]
fn the_standard_endpoint_outranks_the_vendor_alias() {
    let config = resolved([
        (GRAFANA_ENDPOINT_KNOB, VENDOR_ENDPOINT),
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
    ]);

    assert_eq!(&*config.endpoint, STANDARD_ENDPOINT);
    assert_eq!(config.source, OTEL_ENDPOINT_KNOB);
}

/// The vendor's two halves become one basic credential.
#[test]
fn the_vendor_pair_becomes_a_basic_credential() {
    let config = resolved([
        (GRAFANA_ENDPOINT_KNOB, VENDOR_ENDPOINT),
        (GRAFANA_INSTANCE_KNOB, VENDOR_INSTANCE),
        (GRAFANA_API_KEY_KNOB, VENDOR_KEY),
    ]);

    let credential = header(&config, AUTHORIZATION).expect("both halves are configured");
    assert!(
        credential.starts_with("Basic "),
        "the vendor authenticates with a basic credential: {credential}"
    );
    assert!(
        !credential.contains(VENDOR_KEY),
        "the token must be encoded rather than carried in the clear"
    );
}

/// Half a credential is no credential.
///
/// An instance id with no key authenticates nothing, and sending it produces a
/// 401 whose message names nothing an operator can act on.
#[test]
fn half_a_vendor_credential_is_no_credential() {
    for half in [
        vec![(GRAFANA_INSTANCE_KNOB, VENDOR_INSTANCE)],
        vec![(GRAFANA_API_KEY_KNOB, VENDOR_KEY)],
    ] {
        let mut pairs = vec![(GRAFANA_ENDPOINT_KNOB, VENDOR_ENDPOINT)];
        pairs.extend(half);
        let config = resolved(pairs);
        assert_eq!(header(&config, AUTHORIZATION), None);
    }
}

/// A standard header of the same name replaces the vendor's.
///
/// The same precedence the endpoint has, for the same reason: a deployment
/// that has moved to the standard surface must not have a leftover vendor
/// variable deciding what it authenticates with.
#[test]
fn a_standard_header_replaces_the_vendor_credential() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (GRAFANA_INSTANCE_KNOB, VENDOR_INSTANCE),
        (GRAFANA_API_KEY_KNOB, VENDOR_KEY),
        (OTEL_HEADERS_KNOB, "authorization=Bearer collector-token"),
    ]);

    assert_eq!(
        header(&config, AUTHORIZATION).as_deref(),
        Some("Bearer collector-token")
    );
    assert_eq!(
        config.headers.len(),
        1,
        "the replaced credential must not survive beside its replacement"
    );
}

/// Several headers resolve, trimmed, in the order they were written.
#[test]
fn a_header_list_resolves_every_pair() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT),
        (OTEL_HEADERS_KNOB, "x-scope-org-id=tenant-a, x-extra = spaced "),
    ]);

    assert_eq!(header(&config, "x-scope-org-id").as_deref(), Some("tenant-a"));
    assert_eq!(header(&config, "x-extra").as_deref(), Some("spaced"));
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
    ] {
        let mut faults = Vec::new();
        let _config = otlp(
            &MapEnv::from_pairs([(OTEL_ENDPOINT_KNOB, STANDARD_ENDPOINT), (knob, value)]),
            &mut faults,
        );
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

    assert_eq!(&*config.protocol, super::PROTOCOL_PROTOBUF);
    assert_eq!(config.timeout, super::DEFAULT_TIMEOUT);
    assert!(config.headers.is_empty());
}
