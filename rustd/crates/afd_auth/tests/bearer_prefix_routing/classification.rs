//! Which class a presented value routes to, which plane admits it, and what a
//! deployment with no identity provider still resolves.
//!
//! Split from the parent for length; the fixtures every class shares stay
//! there.

use afd_auth::authenticate::Registry;
use afd_auth::capability::NoCapabilitySource;
use afd_auth::credential::{
    CLI_CREDENTIAL_PREFIX, CredentialKind, RUNNER_TOKEN_PREFIX, TENANT_API_KEY_PREFIX,
};
use afd_auth::error::Error;
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::plane::Plane;
use afd_auth::principal::PersonCredential;
use afd_auth::scope::{Scope, parse_claim};
use afd_auth::verifier::NoVerifier;

use super::{BODY, block_on, live_person, present, subject};

// ── Classification ───────────────────────────────────────────────────────

/// No marker shadows another, by rule rather than by authoring order.
///
/// `agt_t` and `agt_r` differ in one byte. A first-match walk over a table that
/// was NOT prefix-free would classify one as the other depending on which row
/// came first; the const assertion in `credential.rs` is what makes that a
/// build failure. This is the runtime half of the same claim.
#[test]
fn test_each_marker_routes_to_exactly_its_own_class() {
    for (marker, expected) in [
        (TENANT_API_KEY_PREFIX, CredentialKind::TenantApiKey),
        (RUNNER_TOKEN_PREFIX, CredentialKind::RunnerToken),
        (CLI_CREDENTIAL_PREFIX, CredentialKind::CliCredential),
    ] {
        let presented = present(&format!("{marker}{BODY}"));
        assert_eq!(
            CredentialKind::of(&presented),
            expected,
            "{marker} must route to {expected:?}"
        );
    }
}

/// A value carrying no marker is the session-token class — which classifies it,
/// and does not accept it.
#[test]
fn test_an_unmarked_value_is_the_session_token_class() {
    assert_eq!(
        CredentialKind::of(&present("eyJhbGciOiJSUzI1NiJ9.e30.sig")),
        CredentialKind::OidcSessionToken
    );
    // `agt_` alone matches no marker, so it is not silently a tenant key.
    assert_eq!(
        CredentialKind::of(&present("agt_something")),
        CredentialKind::OidcSessionToken
    );
}

/// Every class either carries a marker or is the one that carries none, and
/// each marker belongs to exactly one class.
#[test]
fn test_the_prefix_table_and_the_catalogue_agree() {
    let markered: Vec<_> = CredentialKind::ALL
        .into_iter()
        .filter_map(|kind| kind.prefix().map(|marker| (marker, kind)))
        .collect();
    assert_eq!(markered.len(), 3, "exactly one class carries no marker");
    for (marker, kind) in markered {
        assert_eq!(
            CredentialKind::of(&present(&format!("{marker}{BODY}"))),
            kind
        );
    }
}

// ── The plane boundary ───────────────────────────────────────────────────

/// Every class belongs to exactly one plane.
///
/// A class two planes both accept would be a runner token that satisfies a
/// tenant route; a class no plane accepts would be a credential nothing can
/// ever authenticate.
#[test]
fn test_planes_partition_the_catalogue() {
    for kind in CredentialKind::ALL {
        let planes: Vec<_> = Plane::ALL
            .into_iter()
            .filter(|plane| plane.admits(kind))
            .collect();
        assert_eq!(planes.len(), 1, "{kind:?} is admitted by {planes:?}");
    }
}

/// A tenant credential on the runner plane is refused before any lookup, and
/// refused with the RUNNER plane's code.
///
/// `UZ-RUN-001`, not `UZ-AUTH-002`: the runner client classifies its own
/// plane's codes and has no branch for a tenant-plane one.
#[test]
fn test_the_runner_plane_refuses_a_tenant_credential_without_a_lookup() {
    let directory = MockDirectory::new();
    let registry = Registry::new(
        Plane::Runner,
        directory.clone(),
        NoCapabilitySource,
        NoVerifier,
    );

    let refused =
        block_on(registry.authenticate(&present(&format!("{TENANT_API_KEY_PREFIX}{BODY}"))))
            .expect_err("a tenant key must not authenticate on the runner plane");

    assert_eq!(refused, Error::InvalidRunnerToken);
    assert_eq!(refused.code().as_str(), "UZ-RUN-001");
    assert_eq!(
        directory.lookups(),
        0,
        "the refusal must cost no round trip"
    );
}

/// And the mirror: a runner token on the tenant plane, with the TENANT code.
#[test]
fn test_the_tenant_plane_refuses_a_runner_token_without_a_lookup() {
    let directory = MockDirectory::new();
    let registry = Registry::new(
        Plane::Tenant,
        directory.clone(),
        NoCapabilitySource,
        NoVerifier,
    );

    let refused =
        block_on(registry.authenticate(&present(&format!("{RUNNER_TOKEN_PREFIX}{BODY}"))))
            .expect_err("a runner token must not authenticate on the tenant plane");

    assert_eq!(refused, Error::InvalidOrMissingToken);
    assert_eq!(refused.code().as_str(), "UZ-AUTH-002");
    assert_eq!(directory.lookups(), 0);
}

// ── The half of 4.1 that names a deployment ──────────────────────────────

/// The dimension's own wording: a deployment with NO identity provider still
/// resolves `agt_t` and `afc_`.
///
/// The prefixed classes never consult a verifier at all, so there is no
/// statement order for a future edit to disturb — [`NoVerifier`] is the whole
/// configuration and the test still passes.
#[test]
fn test_a_deployment_with_no_identity_provider_still_resolves_both_stored_classes() {
    let tenant_key = present(&format!("{TENANT_API_KEY_PREFIX}{BODY}"));
    let cli_credential = present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"));
    let directory = MockDirectory::new()
        .with(CredentialKind::TenantApiKey, &tenant_key, live_person())
        .with(
            CredentialKind::CliCredential,
            &cli_credential,
            live_person(),
        );
    let capabilities =
        MockCapabilities::new().with(&subject(), parse_claim(Scope::FleetWrite.wire()));
    let registry = Registry::new(Plane::Tenant, directory, capabilities, NoVerifier);

    for (presented, expected) in [
        (&tenant_key, PersonCredential::TenantApiKey),
        (&cli_credential, PersonCredential::CliCredential),
    ] {
        let principal =
            block_on(registry.authenticate(presented)).expect("the class needs no verifier");
        let person = principal.person().expect("a person");
        assert_eq!(*person.credential(), expected);
        // Resolved live from the provider, never granted by the class.
        assert!(principal.scopes().contains(Scope::FleetRead));
    }
}

/// And the class that DOES need one is refused, as a rejection rather than an
/// outage: an operator who never configured an issuer has suffered no outage.
#[test]
fn test_a_deployment_with_no_identity_provider_refuses_a_session_token() {
    let registry = Registry::new(
        Plane::Tenant,
        MockDirectory::new(),
        NoCapabilitySource,
        NoVerifier,
    );

    let refused = block_on(registry.authenticate(&present("eyJhbGciOiJSUzI1NiJ9.e30.sig")))
        .expect_err("no verifier is configured");

    assert_eq!(refused, Error::InvalidOrMissingToken);
    assert!(refused.is_rejection(), "not an outage: nothing is down");
}
