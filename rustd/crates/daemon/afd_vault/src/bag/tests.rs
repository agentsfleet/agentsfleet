#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; an unmet precondition should stop it"
)]

use af_crypto::secret::{SecretBytes, SecretString};
use serde::Deserialize;

use af_core::test_util::trace::Capture;

use super::{Bag, BagFault, filled, parse, parsed, secret, usable};

/// A two-field bag for proving the shared half.
#[derive(Debug)]
struct Probe {
    host: String,
    port: u16,
    token: SecretString,
}

#[derive(Deserialize)]
struct StoredProbe {
    host: Option<String>,
    port: Option<String>,
    token: Option<SecretString>,
}

impl Bag for Probe {
    type Stored = StoredProbe;

    fn from_stored(stored: StoredProbe) -> Result<Self, BagFault> {
        Ok(Self {
            host: filled(stored.host, "host")?,
            port: parsed(stored.port, "port")?,
            token: secret(stored.token, "token")?,
        })
    }
}

fn probe(json: &str) -> Result<Probe, BagFault> {
    parse::<Probe>(&SecretBytes::new(json.as_bytes().to_vec()))
}

#[test]
fn a_complete_bag_parses_into_its_typed_value() {
    let parsed = probe(r#"{"host":"relay.example.test","port":"465","token":"t-1"}"#)
        .expect("a complete bag parses");
    assert_eq!(parsed.host, "relay.example.test");
    assert_eq!(parsed.port, 465);
    assert_eq!(parsed.token.expose(), "t-1");
}

#[test]
fn a_body_that_does_not_decode_is_malformed() {
    for json in [
        "",
        "not json",
        "[]",
        "null",
        r#"["h","1","t"]"#,
        r#"{"host":"h","port":"1","token":"t"} trailing"#,
        r#"{"host":7,"port":"1","token":"t"}"#,
    ] {
        assert_eq!(probe(json).err(), Some(BagFault::Malformed), "{json:?}");
    }
}

#[test]
fn an_absent_or_empty_field_is_missing_by_name() {
    for (json, field) in [
        (r#"{"port":"1","token":"t"}"#, "host"),
        (r#"{"host":"","port":"1","token":"t"}"#, "host"),
        (r#"{"host":"h","token":"t"}"#, "port"),
        (r#"{"host":"h","port":"1"}"#, "token"),
        (r#"{"host":"h","port":"1","token":""}"#, "token"),
    ] {
        assert_eq!(probe(json).err(), Some(BagFault::Missing(field)), "{json}");
    }
}

#[test]
fn a_field_that_is_not_its_type_is_unparsed_by_name() {
    for port in ["\"x\"", "\"65536\"", "\"-1\""] {
        let json = format!(r#"{{"host":"h","port":{port},"token":"t"}}"#);
        assert_eq!(
            probe(&json).err(),
            Some(BagFault::Unparsed("port")),
            "{json}"
        );
    }
}

#[test]
fn unknown_fields_are_ignored_and_non_ascii_survives() {
    let parsed = probe(r#"{"host":"hôte","port":"25","token":"t","note":"rotated"}"#)
        .expect("an extra field is not a fault");
    assert_eq!(parsed.host, "hôte");
}

#[test]
fn a_fault_renders_its_name_and_field_and_never_a_value() {
    for (fault, name, field, rendered) in [
        (BagFault::Absent, "absent", None, "absent"),
        (BagFault::Malformed, "malformed", None, "malformed"),
        (
            BagFault::Missing("token"),
            "missing",
            Some("token"),
            "missing field token",
        ),
        (
            BagFault::Unparsed("port"),
            "unparsed",
            Some("port"),
            "unparsed field port",
        ),
    ] {
        assert_eq!(fault.name(), name);
        assert_eq!(fault.field(), field);
        assert_eq!(fault.to_string(), rendered);
    }
    let debug = format!("{:?}", probe(r#"{"host":"h","port":"1","token":"s3cret"}"#));
    assert!(!debug.contains("s3cret"), "Debug leaked the token: {debug}");
}

const EVENT: &str = "probe_bag_unusable";
const CODE: &str = "UZ-PROBE-001";

fn complete_probe() -> Probe {
    Probe {
        host: "relay.example.test".to_owned(),
        port: 587,
        token: SecretString::new("t-1".to_owned()),
    }
}

#[test]
fn a_usable_bag_is_its_value_and_says_nothing() {
    let capture = Capture::install();

    let found =
        usable(Ok(complete_probe()), "probe", EVENT, CODE).expect("a usable bag yields its value");

    assert_eq!(found.host, "relay.example.test");
    assert!(capture.events().is_empty());
}

#[test]
fn an_absent_bag_is_nothing_and_says_nothing() {
    let capture = Capture::install();

    assert!(usable::<Probe>(Err(BagFault::Absent), "probe", EVENT, CODE).is_none());

    assert!(capture.events().is_empty(), "nobody configured it");
}

#[test]
fn a_bag_that_fails_its_schema_is_nothing_and_names_the_field_at_fault() {
    let capture = Capture::install();

    assert!(usable::<Probe>(Err(BagFault::Missing("token")), "probe", EVENT, CODE).is_none());

    let logged = capture.only(EVENT);
    assert_eq!(logged.field("bag"), Some("probe"));
    assert_eq!(logged.field("fault"), Some("missing"));
    assert_eq!(logged.field("field"), Some("token"));
    assert_eq!(logged.field("error_code"), Some(CODE));
}

#[test]
fn a_bag_that_is_not_a_document_names_no_field() {
    let capture = Capture::install();

    assert!(usable::<Probe>(Err(BagFault::Malformed), "probe", EVENT, CODE).is_none());

    let logged = capture.only(EVENT);
    assert_eq!(logged.field("fault"), Some("malformed"));
    assert_eq!(logged.field("field"), None, "there is no field to name");
}
