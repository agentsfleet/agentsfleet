#![expect(
    clippy::expect_used,
    reason = "validated identity values are test fixture preconditions"
)]

use afd_api::services::SignupMetadata as _;
use afd_auth::capability::{CapabilitySource as _, NoCapabilitySource};
use afd_auth::credential::Presented;
use afd_auth::principal::Subject;
use afd_auth::verifier::{NoVerifier, TokenVerifier as _, VerifyError};
use afd_identity::{ClaimUnavailable, MetadataUnwritten, ProviderSecret};

use super::{
    Capabilities, NoWriteback, Sessions, SignupWriteback, build_claims, build_key_set,
    build_metadata, resolve, resolve_with,
};
use crate::preflight::IdentityConfig;

fn configured() -> IdentityConfig {
    IdentityConfig {
        issuer: "https://identity.example.test".into(),
        audience: "agentsfleet".into(),
        jwks_url: None,
        api_base: "https://api.example.test".into(),
        secret: ProviderSecret::new("fixture-secret").expect("fixture secret is valid"),
    }
}

#[test]
fn complete_configuration_constructs_both_identity_seams() {
    let (capabilities, sessions, writeback) = resolve(&configured());
    assert!(matches!(capabilities, Capabilities::Provider(_)));
    assert!(matches!(sessions, Sessions::Jwks(_)));
    assert!(matches!(writeback, SignupWriteback::Provider(_)));
}

#[test]
fn each_unbuildable_client_degrades_only_its_own_seam() {
    let identity = configured();
    let (capabilities, sessions, writeback) = resolve_with(
        &identity,
        |_identity| Err(ClaimUnavailable::Unreachable),
        build_key_set,
        build_metadata,
    );
    assert!(matches!(capabilities, Capabilities::Unconfigured(_)));
    assert!(matches!(sessions, Sessions::Jwks(_)));
    assert!(matches!(writeback, SignupWriteback::Provider(_)));

    let (capabilities, sessions, writeback) = resolve_with(
        &identity,
        build_claims,
        |_url, _timeout| Err(VerifyError::KeySetUnavailable),
        build_metadata,
    );
    assert!(matches!(capabilities, Capabilities::Provider(_)));
    assert!(matches!(sessions, Sessions::Unconfigured(_)));
    assert!(matches!(writeback, SignupWriteback::Provider(_)));

    // The writeback degrades on its own too: a client it cannot build leaves
    // the capability read and the session verifier untouched.
    let (capabilities, sessions, writeback) =
        resolve_with(&identity, build_claims, build_key_set, |_identity| {
            Err(afd_identity::MetadataUnwritten::Unreachable)
        });
    assert!(matches!(capabilities, Capabilities::Provider(_)));
    assert!(matches!(sessions, Sessions::Jwks(_)));
    assert!(matches!(writeback, SignupWriteback::Unconfigured(_)));
}

#[tokio::test]
async fn unconfigured_dispatch_never_disguises_an_outage_as_empty_authority() {
    let subject = Subject::new("user_fixture").expect("fixture subject is valid");
    let capabilities = Capabilities::Unconfigured(NoCapabilitySource);
    let _unavailable = capabilities
        .capabilities(&subject)
        .await
        .expect_err("an absent capability provider is unavailable");

    let presented = Presented::new("session.fixture.token").expect("fixture token is nonblank");
    let sessions = Sessions::Unconfigured(NoVerifier);
    assert_eq!(
        sessions.verify(&presented).await,
        Err(VerifyError::NotConfigured)
    );
}

#[tokio::test]
async fn configured_session_dispatch_still_refuses_malformed_tokens_locally() {
    let (_capabilities, sessions, _writeback) = resolve(&configured());
    let presented = Presented::new("not-a-jwt").expect("fixture token is nonblank");
    assert_eq!(
        sessions.verify(&presented).await,
        Err(VerifyError::Malformed)
    );
}

#[tokio::test]
async fn a_writeback_with_no_client_refuses_rather_than_reporting_success() {
    // The dispatch arm and `NoWriteback` together. Answering `Ok` here would be
    // the worst available outcome: signup has already committed the tenant row,
    // so a silent success leaves an account whose next token carries no
    // `tenant_id` and no record that anything failed. The refusal is what puts
    // a line in front of the operator who has to repair it by hand.
    let subject = Subject::new("user_fixture").expect("fixture subject is valid");
    let writeback = SignupWriteback::Unconfigured(NoWriteback);
    assert_eq!(
        writeback
            .write_signup(&subject, "tn_fixture", "fleet:admin")
            .await,
        Err(MetadataUnwritten::Unreachable)
    );
}

#[tokio::test]
async fn a_configured_writeback_reports_the_outage_instead_of_swallowing_it() {
    // The Provider arm of the same dispatch. Bound and dropped so the connect
    // is refused immediately rather than waiting out `PROVIDER_TIMEOUT`.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let api_base = format!("http://{}", listener.local_addr().expect("a bound address"));
    drop(listener);

    let identity = IdentityConfig {
        api_base: api_base.into(),
        ..configured()
    };
    let (_capabilities, _sessions, writeback) = resolve(&identity);
    assert!(matches!(writeback, SignupWriteback::Provider(_)));

    let subject = Subject::new("user_fixture").expect("fixture subject is valid");
    assert_eq!(
        writeback
            .write_signup(&subject, "tn_fixture", "fleet:admin")
            .await,
        Err(MetadataUnwritten::Unreachable),
        "a configured client that cannot reach the provider must still say so"
    );
}

#[test]
fn an_issuer_that_derives_no_key_set_url_leaves_only_the_session_seam_unconfigured() {
    // `jwks_url` answers `None` when the issuer trims to nothing, which is the
    // one path into `Sessions::Unconfigured` that never builds a client at all.
    // The other two seams read `api_base`, not the issuer, so they must survive
    // it — the same one-seam-at-a-time degradation the unbuildable clients get.
    let identity = IdentityConfig {
        issuer: "/".into(),
        ..configured()
    };
    let (capabilities, sessions, writeback) = resolve(&identity);
    assert!(matches!(sessions, Sessions::Unconfigured(_)));
    assert!(matches!(capabilities, Capabilities::Provider(_)));
    assert!(matches!(writeback, SignupWriteback::Provider(_)));
}
