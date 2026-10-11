//! The header parse, and the session-token path that reads its capabilities
//! off the token.
//!
//! Split from the parent for length; the fixtures every class shares stay
//! there.

use afd_auth::authenticate::Registry;
use afd_auth::capability::NoCapabilitySource;
use afd_auth::credential::{
    CLI_CREDENTIAL_PREFIX, CredentialKind, RUNNER_TOKEN_PREFIX, TENANT_API_KEY_PREFIX,
};
use afd_auth::directory::{DIGEST_HEX_LEN, Digest};
use afd_auth::error::Error;
use afd_auth::mock::{MockCapabilities, MockDirectory, MockVerifier};
use afd_auth::plane::Plane;
use afd_auth::principal::PersonCredential;
use afd_auth::scope::{Scope, parse_claim};
use afd_auth::verifier::{NoVerifier, VerifiedClaims, VerifyError};
use afd_core::id::Uuid7;

use super::{BODY, TENANT, WORKSPACE, block_on, live_machine, live_person, present, subject, uuid};

// ── The header, and the session-token path ───────────────────────────────

/// Everything a header can be wrong about lands in ONE refusal, so a caller
/// cannot tell "you sent nothing" from "you sent the wrong kind of thing".
#[test]
fn test_every_unusable_header_lands_in_one_refusal() {
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new(),
        NoCapabilitySource,
        NoVerifier,
    );
    for header in [
        "",
        "Bearer",
        "Bearer ",
        "Bearer \t\r\n",
        "bearer abc",
        "Basic abc",
    ] {
        let refused = block_on(registry.authenticate_header(header))
            .expect_err("an unusable header authenticates nothing");
        assert_eq!(refused, Error::InvalidOrMissingToken, "{header:?}");
    }
}

/// A usable header reaches the same decision the parsed credential does.
///
/// The two entry points must not be two procedures: `authenticate_header` only
/// parses, and everything after the parse is the one path already under test.
#[test]
fn test_a_usable_header_reaches_the_same_verdict_as_its_credential() {
    let key = present(&format!("{TENANT_API_KEY_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new().with(CredentialKind::TenantApiKey, &key, live_person()),
        MockCapabilities::new().with(&subject(), parse_claim(Scope::BillingRead.wire())),
        NoVerifier,
    );

    let from_header = block_on(registry.authenticate_header(&format!("Bearer {}", key.expose())))
        .expect("a usable header");
    let from_credential = block_on(registry.authenticate(&key)).expect("the same credential");

    assert_eq!(from_header, from_credential);
    assert!(from_header.scopes().contains(Scope::BillingRead));
}

/// A deployment with no provider SECRET is an outage, not an empty grant.
///
/// [`NoCapabilitySource`] is what an unconfigured provider client IS, and the
/// difference from an empty set is the whole point: an empty set would
/// authenticate the caller and then refuse them at every gate as though they
/// had been narrowed to nothing.
#[test]
fn test_an_unconfigured_provider_is_an_outage_for_a_person_credential() {
    let credential = present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new().with(CredentialKind::CliCredential, &credential, live_person()),
        NoCapabilitySource,
        NoVerifier,
    );

    let refused =
        block_on(registry.authenticate(&credential)).expect_err("no provider is configured");

    assert_eq!(refused, Error::Unavailable);
    assert!(!refused.is_rejection(), "the credential itself proved fine");
}

/// The digest renders in full, and matches what the credential column stores.
///
/// Unlike the credential, a digest is not secret — that is the entire point of
/// storing it instead of the value — so it renders rather than redacting, and a
/// log line naming which row was looked up leaks nothing.
#[test]
fn test_a_digest_renders_as_the_hex_a_credential_column_stores() {
    let digest = Digest::of(&present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}")));

    assert_eq!(digest.to_string(), digest.as_str());
    assert_eq!(digest.as_str().len(), DIGEST_HEX_LEN);
    assert!(
        digest
            .as_str()
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "lower-case hex, the form the credential column stores: {digest}"
    );
    // Over the WHOLE presented value, marker included — that is what the
    // stored digest covers, so hashing the body alone would authenticate
    // nothing.
    assert_ne!(digest, Digest::of(&present(BODY)));
}

/// A verified session token carries its own capability claim, so the provider
/// is never asked — the one class that resolves nothing.
#[test]
fn test_a_session_token_reads_its_capabilities_off_the_token() {
    let capabilities = MockCapabilities::new();
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new(),
        capabilities.clone(),
        MockVerifier::accepting(VerifiedClaims {
            subject: subject(),
            tenant: Some(uuid(TENANT)),
            workspace_scope: Some(uuid(WORKSPACE)),
            scope_claim: Some("fleet:admin".into()),
        }),
    );

    let principal = block_on(registry.authenticate(&present("eyJhbGciOiJSUzI1NiJ9.e30.sig")))
        .expect("a verified token");

    let person = principal.person().expect("a person");
    assert!(matches!(
        person.credential(),
        PersonCredential::SessionToken { .. }
    ));
    // The ceiling reaches the principal only from the claim that can carry one.
    assert_eq!(person.workspace_scope().map(Uuid7::as_str), Some(WORKSPACE));
    // The ladder still expands: `fleet:admin` satisfies a `fleet:read` gate.
    assert!(principal.scopes().contains(Scope::FleetRead));
    assert_eq!(capabilities.resolves(), 0, "the claim rides the credential");
}

/// Expiry keeps its own code; every other token failure is redacted to one.
#[test]
fn test_expiry_is_the_only_token_failure_a_caller_is_told_about() {
    for (reason, expected) in [
        (VerifyError::Expired, Error::TokenExpired),
        (VerifyError::SignatureInvalid, Error::InvalidOrMissingToken),
        (VerifyError::IssuerMismatch, Error::InvalidOrMissingToken),
        (VerifyError::AudienceMismatch, Error::InvalidOrMissingToken),
        (VerifyError::Malformed, Error::InvalidOrMissingToken),
        (
            VerifyError::UnsupportedAlgorithm,
            Error::InvalidOrMissingToken,
        ),
        (VerifyError::MissingKeyId, Error::InvalidOrMissingToken),
        (VerifyError::KeyNotFound, Error::InvalidOrMissingToken),
        (VerifyError::MissingClaim, Error::InvalidOrMissingToken),
        // The one that is not a verdict on the token at all.
        (VerifyError::KeySetUnavailable, Error::Unavailable),
    ] {
        let registry = Registry::new(
            Plane::Tenant,
            MockDirectory::new(),
            NoCapabilitySource,
            MockVerifier::refusing(reason),
        );
        let refused = block_on(registry.authenticate(&present("eyJhbGciOiJSUzI1NiJ9.e30.sig")))
            .expect_err("the verifier refused");
        assert_eq!(refused, expected, "{reason:?}");
    }
}

/// A verified token with no tenant claim proves an identity the daemon has
/// nowhere to put, so it does not authenticate.
#[test]
fn test_a_token_without_a_tenant_claim_does_not_authenticate() {
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new(),
        NoCapabilitySource,
        MockVerifier::accepting(VerifiedClaims {
            subject: subject(),
            tenant: None,
            workspace_scope: None,
            scope_claim: None,
        }),
    );

    let refused = block_on(registry.authenticate(&present("eyJhbGciOiJSUzI1NiJ9.e30.sig")))
        .expect_err("a person with no tenant is not a principal this daemon can build");

    assert_eq!(refused, Error::InvalidOrMissingToken);
}

/// A credential nothing matches is unknown, and unknown is the same sentence a
/// malformed one gets — an attacker learns nothing about which guess was closer.
#[test]
fn test_an_unmatched_credential_is_indistinguishable_from_a_malformed_one() {
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new(),
        NoCapabilitySource,
        NoVerifier,
    );
    let unmatched =
        block_on(registry.authenticate(&present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"))))
            .expect_err("no row matches");
    let malformed =
        block_on(registry.authenticate(&present(&format!("{CLI_CREDENTIAL_PREFIX}short"))))
            .expect_err("wrong shape");

    assert_eq!(unmatched, malformed);
    assert_eq!(unmatched.detail(), malformed.detail());
}

/// A directory answering with the wrong record shape fails CLOSED.
///
/// The one branch that exists because a trait cannot make it unrepresentable:
/// a runner store returning a person row must not mint a principal, and the
/// refusal is the class's own unknown code rather than an outage.
#[test]
fn test_a_directory_answering_the_wrong_shape_fails_closed() {
    let token = present(&format!("{RUNNER_TOKEN_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Runner,
        MockDirectory::new().with(CredentialKind::RunnerToken, &token, live_person()),
        MockCapabilities::new(),
        NoVerifier,
    );

    let refused = block_on(registry.authenticate(&token))
        .expect_err("a person row is not a runner, whatever the store said");

    assert_eq!(refused, Error::InvalidRunnerToken);
}

/// The mirror: a person store answering with a machine row.
#[test]
fn test_a_person_class_refuses_a_machine_record() {
    let key = present(&format!("{TENANT_API_KEY_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new().with(CredentialKind::TenantApiKey, &key, live_machine(false)),
        MockCapabilities::new(),
        NoVerifier,
    );

    let refused = block_on(registry.authenticate(&key))
        .expect_err("a machine row must not become a tenant principal");

    assert_eq!(refused, Error::InvalidOrMissingToken);
}
