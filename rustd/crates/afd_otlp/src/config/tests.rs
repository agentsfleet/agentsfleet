//! The four standard knobs read into a configuration, or refuse by name.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the binaries"
)]

use std::time::Duration;

use afd_core::env::MapEnv;

use super::{
    DEFAULT_TIMEOUT, Encoding, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB,
    OTEL_TIMEOUT_KNOB, OtlpConfig, optional, timeout_from,
};

/// Where a configured deployment sends.
const ENDPOINT: &str = "https://collector.example.test:4318";

/// Resolves `pairs`, asserting an endpoint was configured and accepted.
fn resolved<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> OtlpConfig {
    OtlpConfig::from_env(&MapEnv::from_pairs(pairs))
        .expect("every knob reads")
        .expect("an endpoint is configured, so telemetry resolves")
}

/// A deployment that configured nothing exports nothing, and is not at fault.
#[test]
fn no_endpoint_is_no_export_and_no_fault() {
    let resolved = OtlpConfig::from_env(&MapEnv::from_pairs([])).expect("absent is not a fault");
    assert!(resolved.is_none());
}

/// Unset optional knobs resolve to the documented defaults, and a blank knob
/// is an unset one.
#[test]
fn unset_knobs_resolve_to_the_documented_defaults() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, ENDPOINT),
        (OTEL_PROTOCOL_KNOB, "  "),
        (OTEL_TIMEOUT_KNOB, ""),
    ]);

    assert_eq!(config.encoding(), Encoding::HttpProtobuf);
    assert_eq!(config.timeout(), DEFAULT_TIMEOUT);
    assert_eq!(config.headers(), []);
    assert_eq!(config.source(), OTEL_ENDPOINT_KNOB);
}

/// Both accepted spellings reach the exporter's own two encodings, and the
/// knob's spelling comes back for the line that reports it.
#[test]
fn each_accepted_protocol_maps_to_its_own_encoding() {
    assert_eq!(
        resolved([
            (OTEL_ENDPOINT_KNOB, ENDPOINT),
            (OTEL_PROTOCOL_KNOB, "http/json")
        ])
        .encoding(),
        Encoding::HttpJson
    );
    assert_eq!(Encoding::HttpJson.as_str(), "http/json");
    assert_eq!(Encoding::HttpProtobuf.as_str(), "http/protobuf");
    assert!(matches!(
        Encoding::HttpJson.wire(),
        opentelemetry_otlp::Protocol::HttpJson
    ));
    assert!(matches!(
        Encoding::HttpProtobuf.wire(),
        opentelemetry_otlp::Protocol::HttpBinary
    ));
}

/// A timeout is read as milliseconds, exactly as written.
///
/// The unit is the whole risk here. Reading the same digits as seconds turns
/// a one-and-a-half second budget into twenty-five minutes.
#[test]
fn a_timeout_is_kept_in_the_milliseconds_it_was_written_in() {
    // pin test: literal is the contract — the knob's unit is milliseconds.
    let config = resolved([(OTEL_ENDPOINT_KNOB, ENDPOINT), (OTEL_TIMEOUT_KNOB, "1500")]);

    assert_eq!(config.timeout(), Duration::from_millis(1500));
}

/// Every unusable knob is refused by name, and the refusal reaches the
/// caller as data it can aggregate.
#[test]
fn an_unusable_knob_is_refused_naming_itself() {
    for (knob, value) in [
        // pin test: literal is the contract — `grpc` is the spelling the
        // specification defines and this build refuses.
        (OTEL_PROTOCOL_KNOB, "grpc"),
        (OTEL_PROTOCOL_KNOB, "http/xml"),
        (OTEL_TIMEOUT_KNOB, "0"),
        (OTEL_TIMEOUT_KNOB, "soon"),
        (OTEL_ENDPOINT_KNOB, "not a url"),
    ] {
        let pairs = if knob == OTEL_ENDPOINT_KNOB {
            vec![(knob, value)]
        } else {
            vec![(OTEL_ENDPOINT_KNOB, ENDPOINT), (knob, value)]
        };
        let Err(refused) = OtlpConfig::from_env(&MapEnv::from_pairs(pairs)) else {
            unreachable!("`{knob}={value}` must be refused");
        };
        let refused = refused
            .refused()
            .expect("a refused knob is carried as data");
        assert_eq!(refused.knob, knob, "`{knob}={value}` must name its knob");
        assert!(
            !refused.why.contains(value),
            "the refusal describes what the knob accepts, never what it was set to"
        );
    }
}

/// An endpoint the exporter cannot post to is refused naming its knob, and one
/// it can is kept.
///
/// `http::Uri` alone accepts the first three refused values: a host and port
/// with no scheme, a bare host, and a bare path. Each would build an exporter
/// that posts nothing, or one whose refusal echoes the whole value.
#[test]
fn an_endpoint_the_exporter_cannot_post_to_is_refused() {
    for refused in [
        "collector:4318",
        "localhost",
        "/v1/traces",
        "ftp://collector:4318",
        "http://collector:4318?token=x",
        "http://collector:4318/#signals",
        "user:secret@collector:4318",
    ] {
        assert_eq!(
            OtlpConfig::new(refused, OTEL_ENDPOINT_KNOB).map_err(|refused| refused.knob),
            Err(OTEL_ENDPOINT_KNOB),
            "`{refused}` cannot carry a signal path"
        );
    }
    for kept in [
        ENDPOINT,
        "https://collector.example:4318/otlp",
        "http://user:secret@collector:4318",
    ] {
        assert!(
            OtlpConfig::new(kept, OTEL_ENDPOINT_KNOB).is_ok(),
            "`{kept}` is an endpoint the exporter posts to"
        );
    }
}

/// The parsers are usable one knob at a time, which is how a caller that
/// aggregates faults reads them.
#[test]
fn each_knob_parses_on_its_own() {
    assert_eq!(timeout_from("250"), Ok(Duration::from_millis(250)));
    assert_eq!(
        timeout_from("0").map_err(|refused| refused.knob),
        Err(OTEL_TIMEOUT_KNOB)
    );
    assert_eq!("http/json".parse(), Ok(Encoding::HttpJson));
    assert_eq!(
        "grpc".parse::<Encoding>().map_err(|refused| refused.knob),
        Err(OTEL_PROTOCOL_KNOB)
    );
    let refused = OtlpConfig::new("not a url", "A_KNOB").map(|_config| ());
    assert_eq!(refused.map_err(|refused| refused.knob), Err("A_KNOB"));
}

/// The headers knob is not read here: it is each binary's policy.
#[test]
fn the_headers_knob_is_left_to_the_caller() {
    let config = resolved([
        (OTEL_ENDPOINT_KNOB, ENDPOINT),
        (OTEL_HEADERS_KNOB, "authorization=Bearer token"),
    ]);

    assert_eq!(config.headers(), []);
    let configured = config.with_headers(vec![("x-scope".to_owned(), "a".to_owned())]);
    assert_eq!(configured.headers().len(), 1);
}

/// The builders change one field and keep the rest.
#[test]
fn the_builders_change_one_field_each() {
    let config = OtlpConfig::new(ENDPOINT, OTEL_ENDPOINT_KNOB)
        .expect("an absolute URL")
        .with_encoding(Encoding::HttpJson)
        .with_timeout(Duration::from_secs(2));

    assert_eq!(config.encoding(), Encoding::HttpJson);
    assert_eq!(config.timeout(), Duration::from_secs(2));
    assert_eq!(config.source(), OTEL_ENDPOINT_KNOB);
}

/// A user in the endpoint's authority is seen; one in its path is not.
///
/// The runner refuses the first — it holds no credential — and the second
/// is an ordinary path a collector may well route.
#[test]
fn a_user_in_the_authority_is_seen_and_one_in_the_path_is_not() {
    let named =
        OtlpConfig::new("http://u:p@collector:4318", OTEL_ENDPOINT_KNOB).expect("an absolute URL");
    assert!(named.endpoint_names_a_user());

    let plain = OtlpConfig::new("http://collector:4318/tenant@acme", OTEL_ENDPOINT_KNOB)
        .expect("an absolute URL");
    assert!(!plain.endpoint_names_a_user());

    let bare = OtlpConfig::new(ENDPOINT, OTEL_ENDPOINT_KNOB).expect("an absolute URL");
    assert!(!bare.endpoint_names_a_user());
}

/// A trailing slash on the endpoint does not double the separator.
///
/// `https://host//v1/traces` is a path a collector does not route, and the
/// process that posted it would report success at every layer it owns while
/// nothing arrived.
#[test]
fn a_trailing_slash_does_not_double_the_signal_path() {
    let config =
        OtlpConfig::new("https://collector.example.test/", OTEL_ENDPOINT_KNOB).expect("a URL");

    assert_eq!(
        config.signal_endpoint("/v1/traces"),
        "https://collector.example.test/v1/traces"
    );
}

/// A resolved configuration never renders a header's value, only its name.
#[test]
fn a_resolved_configuration_renders_no_credential() {
    let config = OtlpConfig::new(ENDPOINT, OTEL_ENDPOINT_KNOB)
        .expect("a URL")
        .with_headers(vec![(
            "Authorization".to_owned(),
            "Bearer not-a-real-token".to_owned(),
        )]);

    let rendered = format!("{config:?}");
    assert!(!rendered.contains("not-a-real-token"), "{rendered}");
    assert!(
        !rendered.contains(ENDPOINT),
        "the endpoint value stays out too: {rendered}"
    );
    assert!(
        rendered.contains("Authorization"),
        "the header NAME stays: {rendered}"
    );
    assert!(
        rendered.contains(OTEL_ENDPOINT_KNOB),
        "and the knob's name: {rendered}"
    );
}

/// `optional` treats blank as unset and trims what it keeps.
#[test]
fn optional_trims_and_treats_blank_as_unset() {
    let env = MapEnv::from_pairs([("A", " value "), ("B", "   ")]);

    assert_eq!(optional(&env, "A").as_deref(), Some("value"));
    assert_eq!(optional(&env, "B"), None);
    assert_eq!(optional(&env, "C"), None);
}
