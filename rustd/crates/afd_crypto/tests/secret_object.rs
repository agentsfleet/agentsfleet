//! Secret object edits retain provider fields and never render plaintext.
#![expect(clippy::expect_used, reason = "test prerequisites must fail loudly")]
use afd_crypto::secret::{SecretObject, SecretString};

#[test]
fn rotation_preserves_nested_provider_metadata_and_canonical_order() {
    let mut object = SecretObject::parse(
        br#"{"refresh_token":"old-secret","metadata":{"accounts":["tenant-secret",2,true]}}"#,
    )
    .expect("a stored provider object");
    object.replace_string("refresh_token", "replacement-secret");
    let canonical = object.canonical().expect("serializable fields");
    assert_eq!(canonical.expose(), br#"{"refresh_token":"replacement-secret","metadata":{"accounts":["tenant-secret",2,true]}}"#);
    assert_eq!(format!("{object:?}"), "SecretObject(redacted)");
    assert!(!format!("{canonical:?}").contains("secret"));
}

#[test]
fn invalid_secret_objects_are_refused() {
    for raw in ["[]", "null", "42", "\"secret\"", "{\"secret\":"] {
        SecretObject::parse(raw.as_bytes()).expect_err("a non-object secret is refused");
    }
}

#[test]
fn inserting_and_replacing_nested_values_keeps_unrelated_fields() {
    let mut object =
        SecretObject::parse(br#"{"account":"kept","refresh_token":{"old":["secret"]}}"#)
            .expect("a provider object");
    object.replace_string("access_token", "new-access");
    object.replace_string("refresh_token", "new-refresh");
    let encoded = object.canonical().expect("the edited object serializes");
    let fields: serde_json::Value = serde_json::from_slice(encoded.expose()).expect("valid JSON");
    assert_eq!(
        fields,
        serde_json::json!({
            "account": "kept", "access_token": "new-access", "refresh_token": "new-refresh"
        })
    );
}

#[test]
fn secret_text_is_redacted_and_non_string_values_are_refused() {
    let secret: SecretString = serde_json::from_str("\"credential-secret\"").expect("a string");
    assert_eq!(format!("{secret:?}"), "SecretString(redacted)");
    serde_json::from_str::<SecretString>("{}").expect_err("a secret string is not an object");
}
