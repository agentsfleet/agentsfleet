#![expect(
    clippy::expect_used,
    reason = "validated identity values are test fixture preconditions"
)]

use afd_auth::capability::{CapabilitySource as _, NoCapabilitySource};
use afd_auth::credential::Presented;
use afd_auth::principal::Subject;
use afd_auth::verifier::{NoVerifier, TokenVerifier as _, VerifyError};
use afd_identity::{ClaimUnavailable, ProviderSecret};

use super::{
    Capabilities, Sessions, SignupWriteback, build_claims, build_key_set, build_metadata, resolve,
    resolve_with,
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
