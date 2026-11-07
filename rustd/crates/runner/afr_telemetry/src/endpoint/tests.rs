//! The runner holds no observability credential.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afd_core::env::MapEnv;
use afd_core::error_code;
use afd_otlp::{
    COMPRESSION_KNOBS, HEADER_KNOBS, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB,
};

use super::{Endpoint, NO_COMPRESSION, NO_CREDENTIAL};

/// Where a configured runner exports.
const COLLECTOR: &str = "http://collector:4318";

/// Every header knob the exporter reads, or a user in the endpoint, refuses
/// naming the knob — a signal's own header knob included, since the exporter
/// reads it from the environment itself and prefers it to the general one.
#[test]
fn test_runner_refuses_a_credential() {
    let headers = HEADER_KNOBS.into_iter().flat_map(|knob| {
        [
            (vec![(knob, "a=b"), (OTEL_ENDPOINT_KNOB, COLLECTOR)], knob),
            (vec![(knob, "a=b")], knob),
        ]
    });
    let user = (
        vec![(OTEL_ENDPOINT_KNOB, "http://u:p@collector:4318")],
        OTEL_ENDPOINT_KNOB,
    );
    for (pairs, knob) in headers.chain([user]) {
        let Err(refused) = Endpoint::from_env(&MapEnv::from_pairs(pairs)) else {
            unreachable!("a credential for `{knob}` must refuse `run`");
        };
        assert_eq!(refused.knob(), Some(knob));
        assert_eq!(refused.code(), error_code::STARTUP_ENV_CHECK);
        let rendered = refused.to_string();
        assert!(
            rendered.contains(knob) && rendered.contains(NO_CREDENTIAL),
            "{rendered}"
        );
        assert!(
            !rendered.contains("u:p"),
            "the refusal never echoes the value: {rendered}"
        );
    }
}

/// The knobs the daemon refuses, the runner refuses the same way.
#[test]
fn a_malformed_knob_refuses_naming_itself() {
    let Err(refused) = Endpoint::from_env(&MapEnv::from_pairs([
        (OTEL_ENDPOINT_KNOB, COLLECTOR),
        (OTEL_PROTOCOL_KNOB, "grpc"),
    ])) else {
        unreachable!("gRPC is refused");
    };
    assert_eq!(refused.knob(), Some(OTEL_PROTOCOL_KNOB));
}

/// A compression knob refuses `run` naming itself, endpoint or not: this build
/// compresses nothing, and the exporter's own refusal would name no knob.
#[test]
fn a_compression_knob_refuses_naming_itself() {
    for knob in COMPRESSION_KNOBS {
        for pairs in [
            vec![(knob, "gzip")],
            vec![(knob, "gzip"), (OTEL_ENDPOINT_KNOB, COLLECTOR)],
            // A blank value, as `Environment=KNOB=` writes it: the exporter
            // parses it and refuses to build, naming nothing.
            vec![(knob, ""), (OTEL_ENDPOINT_KNOB, COLLECTOR)],
        ] {
            let Err(refused) = Endpoint::from_env(&MapEnv::from_pairs(pairs)) else {
                unreachable!("`{knob}` must refuse `run`");
            };
            assert_eq!(refused.knob(), Some(knob));
            assert_eq!(refused.code(), error_code::STARTUP_ENV_CHECK);
            assert!(refused.to_string().contains(NO_COMPRESSION), "{refused}");
        }
    }
}

/// No endpoint is no export, and a blank header knob is no header.
#[test]
fn no_endpoint_exports_nothing() {
    let resolved = Endpoint::from_env(&MapEnv::from_pairs([(OTEL_HEADERS_KNOB, "  ")]))
        .expect("a blank header knob is unset");
    assert_eq!(resolved, None);
}

/// A plain endpoint resolves, carrying no header.
#[test]
fn a_plain_endpoint_resolves_with_no_header() {
    let endpoint = Endpoint::from_env(&MapEnv::from_pairs([(OTEL_ENDPOINT_KNOB, COLLECTOR)]))
        .expect("every knob reads")
        .expect("an endpoint is configured");
    assert_eq!(endpoint.config().headers(), []);
    assert_eq!(endpoint.config().source(), OTEL_ENDPOINT_KNOB);
}
