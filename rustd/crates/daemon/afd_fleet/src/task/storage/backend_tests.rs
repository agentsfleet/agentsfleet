//! The R2 access pair as its bag stores it: both halves, within the bound R2
//! issues them under, and nothing else in the object.

use af_crypto::secret::SecretBytes;
use afd_vault::bag::{BagFault, parse};

use super::R2Keys;

fn parsed(json: &str) -> Result<R2Keys, BagFault> {
    parse::<R2Keys>(&SecretBytes::new(json.as_bytes().to_vec()))
}

/// The bag with `id` and `secret` as its two halves.
fn pair(id: &str, secret: &str) -> String {
    format!(r#"{{"access_key_id":"{id}","secret_access_key":"{secret}"}}"#)
}

#[test]
fn a_complete_pair_within_the_bound_is_usable() {
    let keys = parsed(&pair("id-1", "secret-1")).ok();

    assert_eq!(
        keys.as_ref().map(|keys| keys.access_key_id.expose()),
        Some("id-1")
    );
    assert_eq!(
        keys.as_ref().map(|keys| keys.secret_access_key.expose()),
        Some("secret-1")
    );
}

#[test]
fn a_half_missing_or_empty_is_refused_by_its_name() {
    for (json, field) in [
        (r#"{"secret_access_key":"s"}"#.to_owned(), "access_key_id"),
        (r#"{"access_key_id":"i"}"#.to_owned(), "secret_access_key"),
        (pair("", "s"), "access_key_id"),
        (pair("i", ""), "secret_access_key"),
    ] {
        assert_eq!(
            parsed(&json).err(),
            Some(BagFault::Missing(field)),
            "{json}"
        );
    }
}

/// R2 issues neither half past 256 bytes; the bound is declared once, and the
/// fault names the half that is over, the first when both are.
#[test]
fn a_half_past_the_bound_is_refused_by_its_name() {
    let longest = "k".repeat(256);
    let too_long = "k".repeat(257);

    assert_eq!(
        parsed(&pair(&longest, &longest)).err(),
        None,
        "a pair at the bound is usable"
    );
    for (json, field) in [
        (pair(&too_long, "s"), "access_key_id"),
        (pair("i", &too_long), "secret_access_key"),
        (pair(&too_long, &too_long), "access_key_id"),
    ] {
        assert_eq!(
            parsed(&json).err(),
            Some(BagFault::Unparsed(field)),
            "{field}"
        );
    }
}

/// A misspelt half must not be dropped on the way to a pair that looks whole.
#[test]
fn an_unknown_field_or_a_body_that_is_not_an_object_is_malformed() {
    for json in [
        r#"{"access_key_id":"i","secret_access_key":"s","bucket":"b"}"#,
        r#"{"access_key":"i","secret_access_key":"s"}"#,
        r#"{"access_key_id":7,"secret_access_key":"s"}"#,
        "[]",
        "null",
        "not json",
        "",
    ] {
        assert_eq!(parsed(json).err(), Some(BagFault::Malformed), "{json:?}");
    }
}
