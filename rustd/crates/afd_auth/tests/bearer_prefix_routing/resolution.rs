//! One procedure across three classes, and an outage that is never a
//! rejection.
//!
//! Split from the parent for length; the fixtures every class shares stay
//! there.

use afd_auth::authenticate::Registry;
use afd_auth::capability::NoCapabilitySource;
use afd_auth::credential::{
    CLI_CREDENTIAL_PREFIX, CredentialKind, RUNNER_TOKEN_PREFIX, TENANT_API_KEY_PREFIX,
};
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::error::Error;
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::plane::Plane;
use afd_auth::scope::{Scope, ScopeSet};
use afd_auth::verifier::NoVerifier;

use super::{BODY, RUNNER, block_on, live_machine, live_person, present, revoked_person, uuid};

// ── One procedure, three classes ─────────────────────────────────────────

/// A malformed body is refused before the round trip that could only have said
/// the same thing — for all three classes.
#[test]
fn test_a_malformed_body_costs_no_round_trip_in_any_class() {
    for (plane, marker, expected) in [
        (
            Plane::Tenant,
            TENANT_API_KEY_PREFIX,
            Error::InvalidOrMissingToken,
        ),
        (
            Plane::Tenant,
            CLI_CREDENTIAL_PREFIX,
            Error::InvalidOrMissingToken,
        ),
        (
            Plane::Runner,
            RUNNER_TOKEN_PREFIX,
            Error::InvalidRunnerToken,
        ),
    ] {
        let directory = MockDirectory::new();
        let registry = Registry::new(plane, directory.clone(), NoCapabilitySource, NoVerifier);
        for body in ["", "tooshort", &BODY.to_uppercase(), &format!("{BODY}0")] {
            let refused = block_on(registry.authenticate(&present(&format!("{marker}{body}"))))
                .expect_err("a malformed body proves nothing");
            assert_eq!(refused, expected, "{marker}{body}");
        }
        assert_eq!(
            directory.lookups(),
            0,
            "{marker}: a shape refusal must not reach the datastore"
        );
    }
}

/// Each class answers its OWN revocation code from the one shared procedure.
///
/// The differences that were three hand-written bodies are three constants,
/// and this is the assertion that they are still three different constants.
#[test]
fn test_each_class_answers_its_own_revocation_code() {
    for (plane, kind, marker, expected, code) in [
        (
            Plane::Tenant,
            CredentialKind::TenantApiKey,
            TENANT_API_KEY_PREFIX,
            Error::TenantKeyRevoked,
            "UZ-APIKEY-004",
        ),
        (
            Plane::Tenant,
            CredentialKind::CliCredential,
            CLI_CREDENTIAL_PREFIX,
            Error::CliCredentialRevoked,
            "UZ-AUTH-023",
        ),
    ] {
        let presented = present(&format!("{marker}{BODY}"));
        let registry = Registry::new(
            plane,
            MockDirectory::new().with(kind, &presented, revoked_person()),
            MockCapabilities::new(),
            NoVerifier,
        );
        let refused = block_on(registry.authenticate(&presented))
            .expect_err("a revoked credential must not authenticate");
        assert_eq!(refused, expected);
        assert_eq!(refused.code().as_str(), code);
    }

    // The runner's counterpart: cordon, drain, revoke and delete all land here,
    // and this rejection is the only channel by which a runner learns it is out
    // of service.
    let token = present(&format!("{RUNNER_TOKEN_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Runner,
        MockDirectory::new().with(
            CredentialKind::RunnerToken,
            &token,
            CredentialRecord::Machine {
                runner: uuid(RUNNER),
                degraded: false,
                live: Liveness::Revoked,
            },
        ),
        NoCapabilitySource,
        NoVerifier,
    );
    let refused = block_on(registry.authenticate(&token)).expect_err("a cordoned runner");
    assert_eq!(refused, Error::RunnerStateBlocked);
    assert_eq!(refused.code().as_str(), "UZ-RUN-009");
}

/// A runner authenticates to `runner:self` and nothing else, and its degraded
/// verdict rides the lookup onto the principal.
#[test]
fn test_a_runner_token_yields_a_machine_principal_carrying_its_verdict() {
    for degraded in [false, true] {
        let token = present(&format!("{RUNNER_TOKEN_PREFIX}{BODY}"));
        let capabilities = MockCapabilities::new();
        let registry = Registry::new(
            Plane::Runner,
            MockDirectory::new().with(CredentialKind::RunnerToken, &token, live_machine(degraded)),
            capabilities.clone(),
            NoVerifier,
        );

        let principal = block_on(registry.authenticate(&token)).expect("an active runner");
        let runner = principal.runner().expect("a machine principal");
        assert_eq!(runner.id().as_str(), RUNNER);
        assert_eq!(runner.is_degraded(), degraded);
        assert!(principal.tenant().is_none(), "a runner holds no tenant");
        assert_eq!(
            principal.scopes(),
            ScopeSet::from_scopes(&[Scope::RunnerSelf])
        );
        assert_eq!(
            capabilities.resolves(),
            0,
            "a machine has no identity at the provider to ask about"
        );
    }
}

// ── Outage, which is never a rejection ───────────────────────────────────

/// A datastore that cannot answer is `UZ-AUTH-004`, never a rejection.
///
/// The runner client counts consecutive REJECTIONS toward a self-termination
/// ceiling and resets that counter on anything else, so misclassifying here
/// would let a Postgres blip walk a healthy fleet to shutdown.
#[test]
fn test_a_datastore_outage_is_never_an_authentication_rejection() {
    let token = present(&format!("{RUNNER_TOKEN_PREFIX}{BODY}"));
    let directory =
        MockDirectory::new().with(CredentialKind::RunnerToken, &token, live_machine(false));
    directory.set_unavailable(true);
    let registry = Registry::new(Plane::Runner, directory, NoCapabilitySource, NoVerifier);

    let refused = block_on(registry.authenticate(&token)).expect_err("the datastore is down");

    assert_eq!(refused, Error::Unavailable);
    assert_eq!(refused.code().as_str(), "UZ-AUTH-004");
    assert!(
        !refused.is_rejection(),
        "an outage is not a verdict on the caller"
    );
}

/// The provider being unreachable is the same: an outage, never an empty set.
///
/// An empty set would read to an operator as a demotion they never received,
/// and would be indistinguishable from a subject the provider has forgotten.
#[test]
fn test_a_provider_outage_is_an_outage_and_not_an_empty_capability_set() {
    let key = present(&format!("{TENANT_API_KEY_PREFIX}{BODY}"));
    let capabilities = MockCapabilities::new();
    capabilities.set_unavailable(true);
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new().with(CredentialKind::TenantApiKey, &key, live_person()),
        capabilities,
        NoVerifier,
    );

    let refused = block_on(registry.authenticate(&key)).expect_err("the provider is down");

    assert_eq!(refused, Error::Unavailable);
    assert!(!refused.is_rejection());
}

/// A subject the provider does not know resolves to no capabilities — an
/// ANSWER, so the caller authenticates and is refused at every gate by name.
#[test]
fn test_a_subject_the_provider_forgot_authenticates_to_nothing() {
    let credential = present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"));
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new().with(CredentialKind::CliCredential, &credential, live_person()),
        MockCapabilities::new(),
        NoVerifier,
    );

    let principal =
        block_on(registry.authenticate(&credential)).expect("the credential itself is still valid");

    assert!(principal.scopes().is_empty(), "fail closed, by absence");
    assert!(
        afd_auth::require_scope(&principal, &[Scope::FleetRead]).is_err(),
        "every non-empty requirement must refuse"
    );
}
