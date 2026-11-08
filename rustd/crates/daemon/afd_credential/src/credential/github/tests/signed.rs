//! A real mint, signed by a freshly generated App key, against the loopback
//! GitHub: one App JWT signs both calls, and its claims state GitHub's window.
#![expect(
    clippy::expect_used,
    reason = "the loopback server, the generated key and the JWT decode are fixture preconditions"
)]

use serde::Deserialize;
use serde_json::json;

use super::super::api::sign;
use super::super::exchange::{Exchange, mint_at};
use super::fake_github::{
    CREATED, FakeGithub, HOLDS_EVERYTHING, INSTALLATION_ID, NOW, OK, binding, loopback_client,
};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::GithubApp;

/// How far before [`NOW`] the App JWT says it was issued: GitHub's documented
/// one-minute allowance for clock drift.
///
/// Pinned here deliberately rather than read from `api.rs`'s `CLOCK_DRIFT`, so
/// a change to the production constant fails this test instead of moving it.
const JWT_BACKDATE_SECONDS: i64 = 60;

/// How long the App JWT lives from its issue instant: GitHub's documented
/// ten-minute ceiling.
///
/// Pinned here deliberately rather than read from `api.rs`'s `JWT_LIFETIME`,
/// for the same reason as [`JWT_BACKDATE_SECONDS`].
const JWT_LIFETIME_SECONDS: i64 = 600;

/// A freshly generated App key.
///
/// Generated per run rather than committed: a private key in the tree is a
/// secret-scanner finding whatever it signs.
fn app_key() -> rcgen::KeyPair {
    rcgen::KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).expect("the host generates an RSA key")
}

/// The platform App holding `key`, PEM-encoded the way an operator pastes one.
fn app(key: &rcgen::KeyPair) -> GithubApp {
    GithubApp {
        app_id: APP_ID,
        private_key_pem: key.serialize_pem().into(),
    }
}

/// The App id every signed case mints as.
const APP_ID: u64 = 7;

/// Mints through the App's own signed client, rooted at `base`.
async fn signed_mint(app: &GithubApp, base: &str) -> Outcome {
    let handle = json!({"integration": "github", "installation_id": INSTALLATION_ID});
    let binding = binding();
    mint_at(
        Exchange {
            app,
            handle: &handle,
            binding: Some(&binding),
            http: &loopback_client(),
            now: NOW,
        },
        base,
    )
    .await
}

/// The claims an App JWT carries, as GitHub reads them.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Claims {
    iat: i64,
    exp: i64,
    iss: u64,
}

#[tokio::test]
async fn a_usable_app_key_signs_both_calls_with_one_backdated_app_jwt() {
    let token = r#"{"token":"ghs_signed","permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let key = app_key();

    let outcome = signed_mint(&app(&key), &github.base).await;

    let minted = outcome.minted().expect("a signed exchange mints");
    assert_eq!(minted.token.as_str(), "ghs_signed");
    assert_eq!(github.asked_permissions().len(), 1);
    let bearers = github.bearers();
    let [read, posted]: [&String; 2] = bearers
        .iter()
        .collect::<Vec<_>>()
        .try_into()
        .expect("both calls reached GitHub");
    assert_eq!(read, posted, "one JWT signs both calls");
    let jwt = read
        .strip_prefix("Bearer ")
        .expect("the JWT rides as a bearer token");
    let public = jsonwebtoken::DecodingKey::from_rsa_pem(key.public_key_pem().as_bytes())
        .expect("the App key's public half is a PEM");
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    // The fixture clock is fixed in the past, so the wall clock would read the
    // JWT as long expired; what is asserted is the window it states.
    validation.validate_exp = false;
    let claims = jsonwebtoken::decode::<Claims>(jwt, &public, &validation)
        .expect("the JWT verifies under the App's public key")
        .claims;
    assert_eq!(
        claims,
        Claims {
            iat: NOW.as_second() - JWT_BACKDATE_SECONDS,
            exp: NOW.as_second() - JWT_BACKDATE_SECONDS + JWT_LIFETIME_SECONDS,
            iss: APP_ID,
        },
        "backdated a minute for clock drift, and inside GitHub's ten-minute ceiling"
    );
}

#[tokio::test]
async fn an_api_root_that_is_not_a_uri_signs_nothing() {
    assert!(matches!(
        signed_mint(&app(&app_key()), "not a uri").await,
        Outcome::MintFailed(Retry::Permanent)
    ));
}

/// The ceiling GitHub puts on an App JWT's `exp`, counted from its `iat`.
const GITHUB_JWT_CEILING_SECONDS: i64 = 600;

#[test]
fn test_github_app_without_octocrab() {
    // The App JWT as `jsonwebtoken` 11 mints it, with no GitHub SDK between:
    // an RS256 header GitHub accepts, the App as issuer, and a window that
    // opens a minute back and closes inside GitHub's ten-minute ceiling.
    let key = app_key();
    let signing = jsonwebtoken::EncodingKey::from_rsa_pem(key.serialize_pem().as_bytes())
        .expect("the App key is an RSA PEM");

    let jwt = sign(APP_ID, &signing, NOW).expect("a usable key signs");

    let header = jsonwebtoken::decode_header(jwt.as_str()).expect("a JWT header");
    assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    assert_eq!(header.typ.as_deref(), Some("JWT"));
    let public = jsonwebtoken::DecodingKey::from_rsa_pem(key.public_key_pem().as_bytes())
        .expect("the App key's public half is a PEM");
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.validate_exp = false;
    let claims = jsonwebtoken::decode::<Claims>(jwt.as_str(), &public, &validation)
        .expect("the JWT verifies under the App's public key")
        .claims;
    assert_eq!(claims.iss, APP_ID);
    assert_eq!(claims.iat, NOW.as_second() - JWT_BACKDATE_SECONDS);
    assert_eq!(claims.exp - claims.iat, JWT_LIFETIME_SECONDS);
    assert!(
        claims.exp - claims.iat <= GITHUB_JWT_CEILING_SECONDS,
        "GitHub refuses an App JWT whose exp lies more than ten minutes past its iat"
    );
}

#[test]
fn a_key_that_is_not_rsa_cannot_sign_an_app_jwt() {
    // An HMAC key handed to an RS256 header is refused at signing, never sent.
    let hmac = jsonwebtoken::EncodingKey::from_secret(b"not an rsa key");

    assert!(sign(APP_ID, &hmac, NOW).is_none());
}
