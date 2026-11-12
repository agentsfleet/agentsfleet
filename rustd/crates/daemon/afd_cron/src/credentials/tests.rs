#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; an unmet precondition should stop it"
)]

use af_crypto::secret::SecretBytes;
use afd_vault::bag::BagFault;

use super::{Credentials, QSTASH_BAG};
use crate::qstash::API_BASE;

fn stored(json: &str) -> SecretBytes {
    SecretBytes::new(json.as_bytes().to_vec())
}

fn parsed(json: &str) -> Result<Credentials, BagFault> {
    afd_vault::bag::parse::<Credentials>(&stored(json))
}

const FULL: &str = r#"{"token":"tok-1","current_signing_key":"cur-1","next_signing_key":"nxt-1","url":"https://qstash-eu-central-1.upstash.io/v2"}"#;

#[test]
fn the_bag_name_is_the_one_the_sync_playbook_writes() {
    assert_eq!(QSTASH_BAG, "qstash");
}

#[test]
fn a_full_bag_yields_its_token_base_and_both_keys() {
    let credentials = parsed(FULL).expect("a full bag parses");
    assert_eq!(credentials.token.expose(), "tok-1");
    assert_eq!(
        credentials.api_base,
        "https://qstash-eu-central-1.upstash.io/v2"
    );
    assert_eq!(credentials.keys.current.expose(), "cur-1");
    assert_eq!(credentials.keys.next.expose(), "nxt-1");
}

#[test]
fn a_bag_without_a_url_uses_the_vendor_default() {
    let credentials = parsed(r#"{"token":"t","current_signing_key":"c","next_signing_key":"n"}"#)
        .expect("url is optional");
    assert_eq!(credentials.api_base, API_BASE);
}

#[test]
fn an_empty_url_uses_the_vendor_default_and_a_trailing_slash_is_dropped() {
    let empty =
        parsed(r#"{"token":"t","current_signing_key":"c","next_signing_key":"n","url":""}"#)
            .expect("an empty url is no url");
    assert_eq!(empty.api_base, API_BASE);
    let slashed =
        parsed(r#"{"token":"t","current_signing_key":"c","next_signing_key":"n","url":"https://q.example.test/v2/"}"#)
            .expect("a slashed url parses");
    assert_eq!(slashed.api_base, "https://q.example.test/v2");
}

#[test]
fn a_url_that_is_not_a_plain_http_address_is_refused_by_its_name() {
    for url in [
        "not a url",
        "ftp://q.example.test/v2",
        "http://q.example.test/v2",
        "https://q.example.test/v2?region=eu",
        "https://q.example.test/v2#base",
        "https://",
        "mailto:ops@example.test",
    ] {
        let json = format!(
            r#"{{"token":"t","current_signing_key":"c","next_signing_key":"n","url":"{url}"}}"#
        );
        assert_eq!(
            parsed(&json).err(),
            Some(BagFault::Unparsed("url")),
            "{url}"
        );
    }
}

/// Cleartext is for the local emulator, which is on a loopback address.
#[test]
fn a_loopback_base_may_be_plain_http() {
    for url in [
        "http://127.0.0.1:8080/v2",
        "http://localhost:8080/v2",
        "http://[::1]:8080/v2",
    ] {
        let json = format!(
            r#"{{"token":"t","current_signing_key":"c","next_signing_key":"n","url":"{url}"}}"#
        );
        assert!(parsed(&json).is_ok(), "{url}");
    }
}

#[test]
fn each_required_field_absent_is_refused_by_its_name() {
    for (json, field) in [
        (
            r#"{"current_signing_key":"c","next_signing_key":"n"}"#,
            "token",
        ),
        (
            r#"{"token":"t","next_signing_key":"n"}"#,
            "current_signing_key",
        ),
        (
            r#"{"token":"t","current_signing_key":"c"}"#,
            "next_signing_key",
        ),
    ] {
        assert_eq!(parsed(json).err(), Some(BagFault::Missing(field)), "{json}");
    }
}

#[test]
fn each_required_field_empty_is_refused_by_its_name() {
    for (json, field) in [
        (
            r#"{"token":"","current_signing_key":"c","next_signing_key":"n"}"#,
            "token",
        ),
        (
            r#"{"token":"t","current_signing_key":"","next_signing_key":"n"}"#,
            "current_signing_key",
        ),
        (
            r#"{"token":"t","current_signing_key":"c","next_signing_key":""}"#,
            "next_signing_key",
        ),
    ] {
        assert_eq!(parsed(json).err(), Some(BagFault::Missing(field)), "{json}");
    }
}

#[test]
fn a_body_that_is_not_an_object_of_strings_is_malformed() {
    for json in [
        "",
        "not json",
        "[]",
        "null",
        r#"{"token":7,"current_signing_key":"c","next_signing_key":"n"}"#,
        r#"{"token":"t","current_signing_key":"c","next_signing_key":"n","url":3}"#,
    ] {
        assert_eq!(parsed(json).err(), Some(BagFault::Malformed), "{json:?}");
    }
}

#[test]
fn unknown_fields_are_ignored_and_non_ascii_values_survive() {
    let credentials = parsed(
        r#"{"token":"tök","current_signing_key":"c","next_signing_key":"n","note":"rotated"}"#,
    )
    .expect("an extra field is not a fault");
    assert_eq!(credentials.token.expose(), "tök");
}

#[test]
fn credentials_never_print_a_secret() {
    let debug = format!("{:?}", parsed(FULL).expect("a full bag parses"));
    for secret in ["tok-1", "cur-1", "nxt-1"] {
        assert!(!debug.contains(secret), "Debug leaked {secret}: {debug}");
    }
}
