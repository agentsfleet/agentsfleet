#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Kind, Projection};
use serde_json::{Map, Value};

/// Parses a body the way [`crate::SecretBody`] would, for the pure cases.
fn object(json: &str) -> Map<String, Value> {
    match serde_json::from_str(json).expect("the fixture is an object") {
        Value::Object(map) => map,
        _not_an_object => unreachable!("the fixture is written as an object"),
    }
}

#[test]
fn every_kind_round_trips_through_its_stored_spelling() {
    for kind in [Kind::ProviderKey, Kind::CustomEndpoint, Kind::CustomSecret] {
        assert_eq!(Kind::parse(kind.as_str()), Some(kind));
    }
}

#[test]
fn an_unknown_spelling_is_refused_rather_than_defaulted() {
    // The read path degrades an unknown kind to `custom_secret` and says so
    // in its own code. Doing it HERE would hide a newer daemon's vocabulary
    // from the one log line that could report it.
    assert_eq!(Kind::parse("managed_identity"), None);
    assert_eq!(Kind::parse(""), None);
    assert_eq!(Kind::parse("PROVIDER_KEY"), None);
}

#[test]
fn a_named_provider_classifies_as_a_provider_key() {
    let projected = Projection::of(&object(r#"{"provider":"anthropic","api_key":"sk-live"}"#));

    assert_eq!(projected.kind, Kind::ProviderKey);
    assert_eq!(projected.provider.as_deref(), Some("anthropic"));
    assert!(projected.has_key);
    // A provider key never carries an endpoint, whatever the body holds.
    assert_eq!(projected.base_url, None);
}

#[test]
fn a_provider_key_does_not_carry_a_base_url_even_when_one_is_stored() {
    let projected = Projection::of(&object(
        r#"{"provider":"openai","base_url":"https://gw/v1"}"#,
    ));

    assert_eq!(projected.kind, Kind::ProviderKey);
    assert_eq!(projected.base_url, None);
}

#[test]
fn the_openai_compatible_id_classifies_as_a_custom_endpoint() {
    let projected = Projection::of(&object(
        r#"{"provider":"openai-compatible","base_url":"https://gw.example.com:8443/v1"}"#,
    ));

    assert_eq!(projected.kind, Kind::CustomEndpoint);
    assert_eq!(
        projected.base_url.as_deref(),
        Some("https://gw.example.com:8443/v1")
    );
}

#[test]
fn a_missing_or_non_string_provider_is_an_opaque_secret() {
    for body in [
        r#"{"host":"db.internal","api_token":"t"}"#,
        r#"{"provider":42}"#,
        r#"{"provider":null}"#,
        r#"{"provider":{"name":"anthropic"}}"#,
    ] {
        let projected = Projection::of(&object(body));
        assert_eq!(projected.kind, Kind::CustomSecret, "{body}");
        assert_eq!(projected.provider, None, "{body}");
    }
}

#[test]
fn an_opaque_secret_still_reports_whether_it_holds_a_key() {
    // The Models page reports presence for an opaque credential the same
    // way, so `has_key` is not a provider-only descriptor.
    let projected = Projection::of(&object(r#"{"api_key":"sk-live","host":"h"}"#));

    assert_eq!(projected.kind, Kind::CustomSecret);
    assert!(projected.has_key);
}

#[test]
fn an_empty_or_non_string_api_key_does_not_count_as_a_key() {
    for body in [
        r#"{"provider":"openai","api_key":""}"#,
        r#"{"provider":"openai","api_key":null}"#,
        r#"{"provider":"openai","api_key":123}"#,
        r#"{"provider":"openai"}"#,
    ] {
        assert!(!Projection::of(&object(body)).has_key, "{body}");
    }
}

#[test]
fn a_base_url_carrying_userinfo_is_omitted_rather_than_rewritten() {
    // The whole point of the promotion is that a projected field is one any
    // authorized caller already sees. A password inside the URL is not, so
    // the column holds nothing rather than holding the password.
    let projected = Projection::of(&object(
        r#"{"provider":"openai-compatible","base_url":"https://user:pw@gw.example.com:8443/v1"}"#,
    ));

    assert_eq!(projected.kind, Kind::CustomEndpoint);
    assert_eq!(projected.base_url, None);
}

#[test]
fn an_at_sign_after_the_authority_is_an_ordinary_path_byte() {
    for url in [
        "https://gw.example.com/v1/models@latest",
        "https://gw.example.com/v1?tag=a@b",
        "https://gw.example.com/v1#a@b",
    ] {
        let body = format!(r#"{{"provider":"openai-compatible","base_url":"{url}"}}"#);
        let projected = Projection::of(&object(&body));
        assert_eq!(projected.base_url.as_deref(), Some(url), "{url}");
    }
}

#[test]
fn userinfo_a_parser_finds_past_an_empty_authority_is_omitted() {
    // The plain split sees no authority in either; a WHATWG parser skips
    // the extra slash and the tab and reads a user and a password.
    for url in [r"https:///u:p@h/v1", r"https://\t/u:p@h"] {
        let body = format!(r#"{{"provider":"openai-compatible","base_url":"{url}"}}"#);
        assert_eq!(Projection::of(&object(&body)).base_url, None, "{url}");
    }
}

#[test]
fn a_raw_at_sign_in_the_authority_stays_omitted_whatever_a_parser_reads() {
    // A parser reads the backslash as a path separator and the `@` as a
    // path byte; the split still sees it in the authority, and either
    // check firing is enough.
    let projected = Projection::of(&object(
        r#"{"provider":"openai-compatible","base_url":"https://h\\@x/v1"}"#,
    ));

    assert_eq!(projected.base_url, None);
}

#[test]
fn a_base_url_with_no_scheme_is_examined_whole() {
    // No `://` means the entire string is the authority, so userinfo in it
    // is still userinfo.
    let projected = Projection::of(&object(
        r#"{"provider":"openai-compatible","base_url":"user:pw@gw.example.com"}"#,
    ));

    assert_eq!(projected.base_url, None);
}

#[test]
fn a_custom_endpoint_with_no_base_url_carries_none() {
    let projected = Projection::of(&object(r#"{"provider":"openai-compatible"}"#));

    assert_eq!(projected.kind, Kind::CustomEndpoint);
    assert_eq!(projected.base_url, None);
    assert_eq!(projected.provider.as_deref(), Some("openai-compatible"));
}
