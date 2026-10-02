#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use super::object_from_slice;
use serde::Deserialize;

/// Two fields, which is what makes the positional reading reachable at all.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Pair {
    provider: String,
    api_key: String,
}

/// A closed struct, so an unknown key is a refusal rather than ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Closed {
    // Deserialized but never read: the refusal is the subject, not the value.
    #[expect(
        dead_code,
        reason = "the field exists so an unknown SIBLING is refusable"
    )]
    provider: String,
}

/// The field name comes back, so a log can say WHICH key was refused.
#[test]
fn an_unknown_field_refusal_yields_its_field_name() {
    let error = object_from_slice::<Closed>(br#"{"provider":"a","extra":1}"#)
        .expect_err("a closed struct refuses an unknown key");

    assert_eq!(super::unknown_field_of(&error).as_deref(), Some("extra"));
}

/// A type refusal yields NOTHING, because its rendering embeds the value it
/// rejected — here an api key — and these bodies carry credentials.
#[test]
fn a_type_refusal_yields_nothing_so_no_value_can_reach_a_log() {
    let error = object_from_slice::<Pair>(br#"{"provider":1,"api_key":"sk-live-secret"}"#)
        .expect_err("a number is not a string");

    assert!(
        error.to_string().contains('1'),
        "precondition: serde embeds the rejected value"
    );
    assert_eq!(super::unknown_field_of(&error), None);
}

/// Syntax and EOF refusals are not `Data`, so they answer `None` too.
#[test]
fn a_malformed_body_yields_nothing() {
    let syntax = object_from_slice::<Pair>(b"{").expect_err("truncated");

    assert_eq!(super::unknown_field_of(&syntax), None);
}

/// An attacker-chosen name is bounded, because the log line that carries it
/// is bounded and the sender picks the key.
#[test]
fn an_absurdly_long_field_name_is_refused_rather_than_logged() {
    let name = "z".repeat(4096);
    let body = format!(r#"{{"provider":"a","{name}":1}}"#);
    let error =
        object_from_slice::<Closed>(body.as_bytes()).expect_err("a closed struct refuses it");

    assert_eq!(super::unknown_field_of(&error), None);
}

/// The empty key is refused too, so a log line never carries a bare name.
///
/// `serde` renders this one as ``unknown field ` ` `` with nothing between
/// the delimiters, which parses cleanly and yields a name that says nothing.
/// Answering `None` keeps the caller's generic refusal rather than logging
/// `field=""`, which reads as a bug in the daemon rather than a bad request.
#[test]
fn the_empty_field_name_is_refused_rather_than_logged() {
    let error = object_from_slice::<Closed>(br#"{"provider":"a","":1}"#)
        .expect_err("a closed struct refuses the empty key like any other");

    assert_eq!(super::unknown_field_of(&error), None);
}

/// A borrowing shape, so the lifetime the request handlers need is proven
/// rather than assumed.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Borrowed<'a> {
    #[serde(borrow)]
    host_id: &'a str,
}

#[test]
fn an_object_deserializes_exactly_as_serde_json_would() {
    let parsed: Pair =
        object_from_slice(br#"{"provider":"anthropic","api_key":"sk-live"}"#).unwrap();

    assert_eq!(
        parsed,
        Pair {
            provider: "anthropic".to_owned(),
            api_key: "sk-live".to_owned(),
        }
    );
}

#[test]
fn a_positional_array_is_refused_where_serde_json_accepts_it() {
    // The hole, stated as the contrast: the plain call succeeds and fills
    // both fields in declaration order.
    let through_serde: Pair = serde_json::from_slice(br#"["anthropic","sk-live"]"#).unwrap();
    assert_eq!(through_serde.api_key, "sk-live");

    let refused = object_from_slice::<Pair>(br#"["anthropic","sk-live"]"#)
        .expect_err("an array is not an object, however well it lines up");
    // serde's own diagnosis, which names the type it wanted — a message
    // this module could not have written as well itself.
    assert!(
        refused.to_string().contains("invalid type: sequence"),
        "{refused}"
    );
    assert!(refused.to_string().contains("Pair"), "{refused}");
}

#[test]
fn every_other_json_value_is_refused_too() {
    for refused in [
        br#""a string""#.as_slice(),
        b"42".as_slice(),
        b"null".as_slice(),
        b"true".as_slice(),
        b"[]".as_slice(),
        b"".as_slice(),
        // A byte-order mark is not JSON whitespace and `serde_json` says so.
        b"\xef\xbb\xbf{}".as_slice(),
    ] {
        object_from_slice::<Pair>(refused).expect_err("only an object is accepted");
    }
}

#[test]
fn leading_whitespace_is_the_formats_business_not_this_modules() {
    let parsed: Pair =
        object_from_slice(b"  \n\t\r{\"provider\":\"a\",\"api_key\":\"b\"}").unwrap();

    assert_eq!(parsed.provider, "a");
}

#[test]
fn trailing_bytes_are_refused_rather_than_half_read() {
    object_from_slice::<Pair>(br#"{"provider":"a","api_key":"b"} and then some"#)
        .expect_err("a body with trailing content is not one object");
}

#[test]
fn a_borrowing_shape_reads_through_unchanged() {
    // The request bodies this guards are `#[serde(borrow)]`, so an adapter
    // that broke borrowing would be unusable at exactly the call sites that
    // need it most.
    let body = br#"{"host_id":"host-1"}"#;
    let parsed: Borrowed<'_> = object_from_slice(body).unwrap();

    assert_eq!(parsed.host_id, "host-1");
}

#[test]
fn an_object_that_does_not_fit_still_fails_through_serde() {
    // The adapter constrains SHAPE and nothing else — a missing field is
    // serde's to report, with its own message, exactly as before.
    let failure = object_from_slice::<Pair>(br#"{"provider":"anthropic"}"#)
        .expect_err("a missing field is still a failure");

    assert!(
        failure.to_string().contains("api_key"),
        "serde's own diagnosis is preserved: {failure}"
    );
}

/// A lenient outer shape embedding a lenient inner one, which is how a
/// runner-bound policy rides inside an operator's request.
#[derive(Debug, Deserialize)]
struct Assignment {
    policy: Assigned,
}

/// The embedded shape. No `deny_unknown_fields`: the runner reads it.
#[derive(Debug, Deserialize)]
struct Assigned {
    worker_count: u32,
}

#[test]
fn the_strict_reader_refuses_a_nested_key_the_type_would_ignore() {
    let body = br#"{"policy":{"worker_count":2,"wroker_count":3}}"#;
    let lenient: Assignment = object_from_slice(body).unwrap();
    assert_eq!(
        lenient.policy.worker_count, 2,
        "precondition: the type ignores it"
    );

    let refused = super::strict_object_from_slice::<Assignment>(body)
        .expect_err("a person's misspelled key is refused, not dropped");

    assert_eq!(
        super::unknown_field_of(&refused).as_deref(),
        Some("policy.wroker_count"),
        "the refusal names the key with its path"
    );
}

#[test]
fn the_strict_reader_reads_an_exact_body() {
    let parsed: Assignment =
        super::strict_object_from_slice(br#"{"policy":{"worker_count":4}}"#).unwrap();

    assert_eq!(parsed.policy.worker_count, 4);
}

#[test]
fn the_strict_reader_keeps_the_object_and_trailing_byte_refusals() {
    super::strict_object_from_slice::<Pair>(br#"["anthropic","sk-live"]"#)
        .expect_err("an array is still not an object");
    super::strict_object_from_slice::<Pair>(br#"{"provider":"a","api_key":"b"} more"#)
        .expect_err("trailing content is still refused");
}
