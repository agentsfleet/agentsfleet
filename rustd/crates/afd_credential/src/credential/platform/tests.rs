//! That a credential is reachable by NAME, and that none of them prints.
#![expect(
    clippy::expect_used,
    reason = "test fixtures should fail loudly when their preconditions drift"
)]
use super::{GithubApp, OauthApp, Platform, github_app, oauth_app, unusable};

fn oauth() -> OauthApp {
    OauthApp {
        client_id: "client-public".to_owned(),
        client_secret: "client-secret-value".to_owned().into(),
    }
}

#[test]
fn test_an_oauth_client_is_reached_by_connector_name() {
    // The property that removes `selectZoho`/`selectJira`/`selectLinear`: a
    // provider added to `connector::DECLARED` is reachable here with no code
    // written for it.
    let platform = Platform::empty()
        .with_oauth("zoho", oauth())
        .with_oauth("a-provider-invented-after-this-test", oauth());

    assert!(platform.oauth("zoho").is_some());
    assert!(
        platform
            .oauth("a-provider-invented-after-this-test")
            .is_some(),
        "a new connector must need no field, no selector and no edit here"
    );
    assert!(platform.oauth("jira").is_none());
}

#[test]
fn test_an_empty_platform_holds_nothing() {
    let platform = Platform::empty();

    assert!(platform.github().is_none());
    assert!(platform.oauth("zoho").is_none());
}

#[test]
fn test_no_secret_reaches_a_debug_line() {
    // `Debug` is what a `tracing` field, a panic message and an `unwrap`
    // expectation all render through, so a derived one would leak the signing
    // key to whichever of those fired first (RULE VLT).
    let platform = Platform::empty()
        .with_github(GithubApp {
            app_id: 7,
            private_key_pem: "-----BEGIN RSA PRIVATE KEY-----SECRET".to_owned().into(),
        })
        .with_oauth("zoho", oauth());

    let rendered = format!("{platform:?}");

    assert!(!rendered.contains("SECRET"), "{rendered}");
    assert!(!rendered.contains("client-secret-value"), "{rendered}");
    // The non-secret halves still render, or the type would be useless to debug.
    assert!(rendered.contains('7'), "{rendered}");
    assert!(rendered.contains("client-public"), "{rendered}");
}

#[test]
fn stored_platform_rows_are_strict_about_required_fields() {
    let github = github_app(br#"{"app_id":"42","private_key_pem":"pem"}"#)
        .expect("a complete GitHub row parses");
    assert_eq!(github.app_id, 42);
    assert_eq!(github.private_key_pem.as_str(), "pem");

    let oauth = oauth_app(br#"{"client_id":"public","client_secret":"secret"}"#)
        .expect("a complete OAuth row parses");
    assert_eq!(oauth.client_id, "public");
    assert_eq!(oauth.client_secret.as_str(), "secret");

    for invalid in [
        b"not-json".as_slice(),
        br#"{"app_id":"not-a-number","private_key_pem":"pem"}"#,
        br#"{"app_id":"42"}"#,
    ] {
        assert!(github_app(invalid).is_none());
    }
    for invalid in [b"not-json".as_slice(), br#"{"client_id":"public"}"#] {
        assert!(oauth_app(invalid).is_none());
    }
}

#[test]
fn unusable_platform_rows_emit_their_diagnostic() {
    let subscriber = tracing_subscriber::fmt().with_test_writer().finish();
    tracing::subscriber::with_default(subscriber, || unusable("github-app"));
}
