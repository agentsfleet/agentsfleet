//! That a credential is reachable by NAME, and that none of them prints.
use super::{GithubApp, OauthApp, Platform};

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
