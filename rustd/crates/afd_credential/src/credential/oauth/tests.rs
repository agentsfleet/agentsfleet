//! The refresh exchange's decisions, none of which need a socket.
//!
//! What the network would contribute is a status and a body, and both are
//! parameters here. The exchange itself is `reqwest`'s and is not what these
//! prove — what they prove is that a hostile or broken answer never becomes a
//! delivered credential, and that the form leaving this daemon is shaped the
//! way a token endpoint reads it.
#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    Answered, DEFAULT_ACCESS_TTL, Grant, Refresh, classify, endpoint, granted, mint, rotated,
};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::OauthApp;

/// The instant every expiry here is measured from.
const NOW_MS: i64 = 1_760_000_000_000;

/// A response body, as a token endpoint writes one.
fn answered(body: &Value) -> Answered {
    serde_json::from_value(body.clone()).expect("the fixture body is well formed")
}

/// The handle object a mint reads, from a JSON literal.
fn handle(value: &Value) -> serde_json::Map<String, Value> {
    value.as_object().expect("a handle is an object").clone()
}

/// Zoho's declared endpoint, standing in for whatever the descriptor carries.
const DECLARED: &str = "https://accounts.zoho.com/oauth/v2/token";

/// An hour, as a provider states it and as this daemon stores it.
///
/// The pair, named (RULE UFS): `expires_in` is SECONDS on the wire and the
/// minted expiry is MILLISECONDS, so the two numbers must move together and a
/// literal repeated at four sites is four places for that relationship to be
/// edited apart.
const EXPIRES_IN_AN_HOUR: i64 = 3_600;
const AN_HOUR_MS: i64 = 3_600_000;

#[test]
fn the_grant_is_form_encoded_by_the_crate_that_owns_the_encoding() {
    // The Zig's own test, kept: these values are provider-issued opaque bytes,
    // and every one of `+ & = %` changes the shape of the form if it escapes
    // its field. The expected string is byte-identical to
    // `integration_oauth_refresh.zig`'s.
    // Built through `reqwest` rather than through the encoder directly, so what
    // is asserted is the request this daemon would actually send.
    let request = reqwest::Client::builder()
        .build()
        .expect("a default client builds")
        .post(DECLARED)
        .form(&Grant {
            grant_type: "refresh_token",
            refresh_token: "rt+a&b=c%",
            client_id: "cid+1",
            client_secret: "sec&ret=",
        })
        .build()
        .expect("a flat struct of strings encodes");

    let body = request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .expect("the form is an in-memory body");
    assert_eq!(
        std::str::from_utf8(body).expect("form encoding is ASCII"),
        "grant_type=refresh_token&refresh_token=rt%2Ba%26b%3Dc%25\
         &client_id=cid%2B1&client_secret=sec%26ret%3D"
    );
    // And the content type the token endpoint parses by, set by the same call.
    assert_eq!(
        request
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(reqwest::header::HeaderValue::as_bytes),
        Some(b"application/x-www-form-urlencoded".as_slice())
    );
}

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
fn a_base_this_daemon_will_not_dial_refuses_before_anything_is_posted() {
    // Each of these would send THIS DEPLOYMENT'S `client_secret` somewhere it
    // must never go, and the Zig checks none of them — it validates the path
    // and takes the base as written. They are one test because they must all
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

#[test]
fn a_success_body_mints_for_exactly_as_long_as_it_states() {
    let outcome = granted(
        &answered(&json!({"access_token": "at_live", "expires_in": EXPIRES_IN_AN_HOUR})),
        "rt_posted",
        NOW_MS,
    );
    let minted = outcome.minted().expect("a success body mints");
    assert_eq!(minted.token.as_str(), "at_live");
    assert_eq!(minted.expires_at_ms, NOW_MS + AN_HOUR_MS);
    assert!(minted.rotated_refresh_token.is_none());
}

#[test]
fn expires_in_is_read_however_the_provider_spells_a_number() {
    // Real OAuth servers emit all three, and a rendering this daemon refuses is
    // a mint that worked before the hostile-input hardening and stops after it.
    for (spelled, expected_ms) in [
        (json!(EXPIRES_IN_AN_HOUR), AN_HOUR_MS),
        // Spelled from the same constant rather than written out: the claim is
        // that the RENDERING varies, not the number, and a literal "3600" here
        // could drift from the row above and still look like it agreed.
        (json!(EXPIRES_IN_AN_HOUR.to_string()), AN_HOUR_MS),
        (json!(1800.5), 1_800_500),
    ] {
        let outcome = granted(
            &answered(&json!({"access_token": "at", "expires_in": spelled})),
            "rt",
            NOW_MS,
        );
        assert_eq!(
            outcome
                .minted()
                .expect("a success body mints")
                .expires_at_ms,
            NOW_MS + expected_ms,
            "{spelled}"
        );
    }
}

#[test]
fn an_absent_expiry_falls_to_the_conservative_floor() {
    // Short, never long: an assumed lifetime past the real one caches a dead
    // token and a child meets a 401 mid-run.
    let outcome = granted(&answered(&json!({"access_token": "at"})), "rt", NOW_MS);
    let expected = i64::try_from(DEFAULT_ACCESS_TTL.as_millis()).unwrap();
    assert_eq!(
        outcome
            .minted()
            .expect("a success body mints")
            .expires_at_ms,
        NOW_MS + expected
    );
}

#[test]
fn a_success_body_this_daemon_cannot_read_mints_nothing() {
    // Every one of these arrives under a 200. A stated expiry that is
    // unreadable is NOT the absent case — falling back to the default there
    // would be believing a body we just decided is malformed.
    for malformed in [
        // The one field the exchange exists to obtain.
        json!({"expires_in": EXPIRES_IN_AN_HOUR}),
        // Negative, and beyond the ten-year ceiling: an expiry nothing real
        // states, which would park a dead token in the broker's cache.
        json!({"access_token": "at", "expires_in": -1}),
        json!({"access_token": "at", "expires_in": 400 * 365 * 24 * 60 * 60_i64}),
        // Non-finite, where the Zig's `@intFromFloat` would have trapped.
        json!({"access_token": "at", "expires_in": 1e30}),
    ] {
        let outcome = granted(&answered(&malformed), "rt", NOW_MS);
        assert!(
            matches!(outcome, Outcome::MintFailed(Retry::Permanent)),
            "{malformed}: {outcome:?}"
        );
    }

    // And a shape that is not a number at all fails the deserialisation
    // itself, which is the same malformed-body path one level up.
    assert!(
        serde_json::from_value::<Answered>(json!({"access_token": "at", "expires_in": {}}))
            .is_err()
    );
}

#[test]
fn only_a_genuinely_new_refresh_token_is_written_back() {
    // A rotation the caller misses costs the tenant the whole connection: these
    // providers invalidate the posted token the moment they issue a successor.
    let rotation = rotated(
        &answered(&json!({"access_token": "at", "refresh_token": "rt_new"})),
        "rt_old",
    );
    assert_eq!(rotation.as_deref().map(String::as_str), Some("rt_new"));

    // An echo is not a rotation, an empty replacement is a broken provider, and
    // silence is a provider that does not rotate. All three write back nothing.
    for quiet in [
        json!({"access_token": "at", "refresh_token": "rt_old"}),
        json!({"access_token": "at", "refresh_token": ""}),
        json!({"access_token": "at"}),
    ] {
        assert!(rotated(&answered(&quiet), "rt_old").is_none(), "{quiet}");
    }
}

#[test]
fn a_dead_refresh_token_is_a_reconnect_at_whatever_status_carries_it() {
    // Providers disagree about the status; they agree about the code, and what
    // it means does not depend on the number in front of it.
    for status in [
        reqwest::StatusCode::BAD_REQUEST,
        reqwest::StatusCode::UNAUTHORIZED,
        reqwest::StatusCode::FORBIDDEN,
    ] {
        let outcome = classify(status, Some(&answered(&json!({"error": "invalid_grant"}))));
        assert!(matches!(outcome, Outcome::ReconnectRequired), "{status}");
    }
}

#[test]
fn every_other_failure_splits_by_whose_fault_it_is() {
    // The GitHub mint's split, and the caller's backoff depends on it: the
    // vendor's fault is worth retrying and our request's is not.
    let vendor = classify(reqwest::StatusCode::BAD_GATEWAY, None);
    assert!(matches!(vendor, Outcome::MintFailed(Retry::Transient)));

    for ours in [
        // Bad client credentials.
        Some(answered(&json!({"error": "invalid_client"}))),
        // A body that told us nothing.
        None,
    ] {
        let outcome = classify(reqwest::StatusCode::UNAUTHORIZED, ours.as_ref());
        assert!(matches!(outcome, Outcome::MintFailed(Retry::Permanent)));
    }
}

#[tokio::test]
async fn a_handle_that_names_no_refresh_token_never_reaches_the_network() {
    // The client here would fail to connect if anything dialled through it, so
    // the assertion is also a proof that nothing did.
    let http = reqwest::Client::builder()
        .build()
        .expect("a default client builds");
    let app = OauthApp {
        client_id: "cid".to_owned(),
        client_secret: Zeroizing::new("secret".to_owned()),
    };

    // No refresh token: a connection a human finishes, not a failure to retry.
    let stored = json!({"integration": "zoho"});
    let outcome = mint(Refresh {
        app: &app,
        handle: &stored,
        token_url: DECLARED,
        http: &http,
        now_ms: NOW_MS,
    })
    .await;
    assert!(matches!(outcome, Outcome::ReconnectRequired));

    // A handle that is not an object names nothing at all.
    let stored = json!(["zoho"]);
    let outcome = mint(Refresh {
        app: &app,
        handle: &stored,
        token_url: DECLARED,
        http: &http,
        now_ms: NOW_MS,
    })
    .await;
    assert!(matches!(outcome, Outcome::MintFailed(Retry::Permanent)));

    // And a base this daemon will not dial fails BEFORE the client secret is
    // put on a wire, which is the whole point of checking it.
    let stored = json!({"refresh_token": "rt", "accounts_base": "http://169.254.169.254"});
    let outcome = mint(Refresh {
        app: &app,
        handle: &stored,
        token_url: DECLARED,
        http: &http,
        now_ms: NOW_MS,
    })
    .await;
    assert!(matches!(outcome, Outcome::MintFailed(Retry::Permanent)));
}

mod transport;
