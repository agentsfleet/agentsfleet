//! Where a refresh is posted, as a stored handle steers it.
//!
//! Split from the exchange's own suite by concern: these read two handle
//! fields and compose an address, and none of them posts anything.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use serde_json::{Value, json};

use super::{Steer, endpoint, steer};

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
fn an_accounts_base_moves_the_post_to_that_data_centre() {
    // The property this field exists for: a token issued in the EU is
    // redeemable at the EU accounts server and nowhere else.
    let resolved = endpoint(
        &handle(&json!({"refresh_token": "rt", "accounts_base": "https://accounts.zoho.eu"})),
        DECLARED,
    );
    assert_eq!(
        resolved.as_deref(),
        Some("https://accounts.zoho.eu/oauth/v2/token")
    );

    // And a handle may name its own path, which is what keeps a provider that
    // spells it differently from being a fourth exchange variant.
    let resolved = endpoint(
        &handle(&json!({
            "refresh_token": "rt",
            "accounts_base": "https://auth.example.com",
            "token_path": "/oauth/token",
        })),
        DECLARED,
    );
    assert_eq!(
        resolved.as_deref(),
        Some("https://auth.example.com/oauth/token")
    );
}

#[test]
fn steer_reads_the_two_fields_the_post_is_composed_from() {
    // The mint admission's view of the handle. It must be the address
    // `endpoint` composes from, or a handle could pass one and post to another.
    assert_eq!(steer(&json!({"refresh_token": "rt"})), Steer::Declared);
    assert_eq!(
        steer(&json!({"accounts_base": "https://accounts.zoho.eu"})),
        Steer::To {
            base: "https://accounts.zoho.eu",
            path: None
        }
    );
    assert_eq!(
        steer(&json!({"accounts_base": "https://a.example", "token_path": "/t"})),
        Steer::To {
            base: "https://a.example",
            path: Some("/t")
        }
    );
    // Not strings, so not overrides — exactly as `endpoint` reads them.
    assert_eq!(steer(&json!({"accounts_base": 7})), Steer::Declared);
    assert_eq!(
        steer(&json!({"accounts_base": "https://a.example", "token_path": 7})),
        Steer::To {
            base: "https://a.example",
            path: None
        }
    );
    assert_eq!(steer(&json!(["accounts_base"])), Steer::Declared);
}

#[test]
fn a_base_this_daemon_will_not_dial_refuses_before_anything_is_posted() {
    // Each of these would send THIS DEPLOYMENT'S `client_secret` somewhere it
    // must never go, and checking only the appended path would let every one
    // through. They are one test because they must all
    // keep answering the same way: a guard safe for four of five is not one.
    for hostile in [
        // Plaintext: the client secret on the wire in the clear.
        json!({"accounts_base": "http://accounts.zoho.eu"}),
        // The cloud metadata service.
        json!({"accounts_base": "https://169.254.169.254"}),
        // Loopback, where an admin port lives.
        json!({"accounts_base": "https://127.0.0.1:8443"}),
        // An internal RFC1918 host.
        json!({"accounts_base": "https://10.1.2.3"}),
        // Not a URL at all.
        json!({"accounts_base": "accounts.zoho.eu"}),
    ] {
        assert!(endpoint(&handle(&hostile), DECLARED).is_none(), "{hostile}");
    }
}

#[test]
fn a_token_path_cannot_smuggle_a_query_or_a_fragment() {
    // `isValidTokenPath`'s reason for existing, asked of the PARSE rather than
    // of the bytes: a handle writer must not be able to widen or redirect where
    // platform credentials are posted.
    for smuggled in ["/oauth/token?redirect=https://evil.test", "/oauth/token#x"] {
        let stored = json!({"accounts_base": "https://auth.example.com", "token_path": smuggled});
        assert!(endpoint(&handle(&stored), DECLARED).is_none(), "{smuggled}");
    }
}
