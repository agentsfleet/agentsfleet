//! Key metadata is a restriction on use, not an ignored provider annotation.
#![expect(clippy::expect_used, reason = "test prerequisites must fail loudly")]

use afd_auth::verifier::VerifyError;
use afd_identity::JwkKeySet;
use serde_json::{Value, json};

use crate::support::signing;

fn key() -> Value {
    let document: Value = serde_json::from_str(&signing::key_set()).expect("a fixture key set");
    document
        .get("keys")
        .and_then(Value::as_array)
        .and_then(|keys| keys.first())
        .expect("one key")
        .clone()
}

#[test]
fn missing_key_type_is_rejected_without_discarding_other_usable_keys() {
    let mut invalid = key();
    let object = invalid.as_object_mut().expect("a key object");
    object.remove("kty");
    object.insert("kid".into(), json!("missing-type"));
    let document = json!({"keys": [key(), invalid]}).to_string();
    let parsed = JwkKeySet::parse(document.as_bytes()).expect("one key remains usable");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed.rejected(), 1);
    assert!(parsed.find("missing-type").is_none());
}

#[test]
fn keys_with_incompatible_usage_or_algorithm_cannot_verify_tokens() {
    for (field, restriction) in [
        ("use", json!("enc")),
        ("alg", json!("RS512")),
        ("alg", json!("HS256")),
        ("key_ops", json!(["encrypt"])),
        ("key_ops", json!([])),
    ] {
        let mut invalid = key();
        invalid
            .as_object_mut()
            .expect("a key object")
            .insert(field.into(), restriction);
        let document = json!({"keys": [invalid]}).to_string();
        assert_eq!(
            JwkKeySet::parse(document.as_bytes()).expect_err(field),
            VerifyError::KeySetUnavailable
        );
    }
}

#[test]
fn ambiguous_key_identifiers_are_unavailable() {
    let document = json!({"keys": [key(), key()]}).to_string();
    assert_eq!(
        JwkKeySet::parse(document.as_bytes()).expect_err("ambiguous key selection"),
        VerifyError::KeySetUnavailable
    );
}

#[test]
fn compatible_optional_metadata_keeps_the_signing_key_usable() {
    let mut explicit = key();
    let fields = explicit.as_object_mut().expect("a key object");
    fields.insert("use".into(), json!("sig"));
    fields.insert("alg".into(), json!("RS256"));
    fields.insert("key_ops".into(), json!(["verify"]));
    for key in [key(), explicit] {
        let document = json!({"keys": [key]}).to_string();
        let parsed = JwkKeySet::parse(document.as_bytes()).expect("a signing key");
        assert!(parsed.find(signing::KID).is_some());
        assert_eq!(parsed.rejected(), 0);
    }
}

#[test]
fn duplicate_key_members_do_not_select_the_last_value() {
    let valid = key().to_string();
    for duplicate in [
        r#""kty":"EC","#,
        r#""kid":"other-key","#,
        r#""n":"AQAB","#,
        r#""e":"AQAB","#,
    ] {
        let malformed = format!(
            "{{{duplicate}{}",
            valid.strip_prefix('{').expect("an object")
        );
        let document = format!(r#"{{"keys":[{malformed}]}}"#);
        assert_eq!(
            JwkKeySet::parse(document.as_bytes()).expect_err("duplicate key metadata"),
            VerifyError::KeySetUnavailable
        );
    }
}
