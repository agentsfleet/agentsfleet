//! Where a refresh is posted, as a stored handle steers it.
//!
//! Split from the exchange's own suite by concern: these read one handle field
//! and compose an address, and none of them posts anything.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::credential::ZOHO_ACCOUNTS_SERVERS;
use serde_json::{Value, json};

use super::{Steer, endpoint, is_zoho_accounts_server, steer};

/// The handle object a mint reads, from a JSON literal.
fn handle(value: &Value) -> serde_json::Map<String, Value> {
    value.as_object().expect("a handle is an object").clone()
}

/// Zoho's declared endpoint, standing in for whatever the descriptor carries.
const DECLARED: &str = "https://accounts.zoho.com/oauth/v2/token";

#[test]
fn a_handle_with_no_accounts_base_posts_where_the_descriptor_says() {
    let resolved = endpoint(&handle(&json!({"refresh_token": "rt"})), DECLARED);
    assert_eq!(resolved.as_deref(), Some(DECLARED));
}

#[test]
fn every_accounts_server_moves_the_post_to_that_data_centre() {
    // The property this field exists for: a token issued in the EU is
    // redeemable at the EU accounts server and nowhere else.
    for base in ZOHO_ACCOUNTS_SERVERS {
        let resolved = endpoint(
            &handle(&json!({"refresh_token": "rt", "accounts_base": base})),
            DECLARED,
        );
        assert_eq!(
            resolved.as_deref(),
            Some(format!("{base}/oauth/v2/token").as_str())
        );
    }
}

#[test]
fn a_base_no_callback_writes_refuses_before_anything_is_posted() {
    // Each would send THIS DEPLOYMENT'S `client_secret` somewhere it must
    // never go. The public host and the lookalike are the cases an SSRF guard
    // admits, and the reason the check is an exact set rather than a shape.
    for hostile in [
        "https://collector.example",
        "https://accounts.zoho.com.collector.example",
        "https://accounts.zoho.com/",
        "https://ACCOUNTS.ZOHO.COM",
        "http://accounts.zoho.eu",
        "https://169.254.169.254",
        "https://127.0.0.1:8443",
        "accounts.zoho.eu",
        "",
    ] {
        let stored = json!({"accounts_base": hostile});
        assert!(endpoint(&handle(&stored), DECLARED).is_none(), "{hostile}");
        assert!(!is_zoho_accounts_server(hostile), "{hostile}");
    }
}

#[test]
fn a_path_override_moves_nothing() {
    // No connect flow ever wrote a path, so none is read: a handle carrying one
    // posts to its accounts server's own token path, never to the one it names.
    let stored = json!({
        "accounts_base": "https://accounts.zoho.eu",
        "token_path": "/collect?then=https://collector.example",
    });
    assert_eq!(
        endpoint(&handle(&stored), DECLARED).as_deref(),
        Some("https://accounts.zoho.eu/oauth/v2/token")
    );
}

#[test]
fn steer_reads_the_field_the_post_is_composed_from() {
    // The mint admission's view of the handle. It must be the address
    // `endpoint` composes from, or a handle could pass one and post to another.
    assert_eq!(steer(&json!({"refresh_token": "rt"})), Steer::Declared);
    assert_eq!(
        steer(&json!({"accounts_base": "https://accounts.zoho.eu"})),
        Steer::To("https://accounts.zoho.eu")
    );
    // Not a string, so not an override — exactly as `endpoint` reads it.
    assert_eq!(steer(&json!({"accounts_base": 7})), Steer::Declared);
    assert_eq!(
        endpoint(&handle(&json!({"accounts_base": 7})), DECLARED).as_deref(),
        Some(DECLARED)
    );
    assert_eq!(steer(&json!(["accounts_base"])), Steer::Declared);
}
