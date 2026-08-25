//! Dimension 4.1 — which credential class a presented value routes to, and
//! what each class does with it.
//!
//! The Zig daemon proves this with three `MockLookup` types in three test
//! files, each exercising one middleware. The routing DECISION — which of them
//! runs — is covered by `bearer_or_api_key_test.zig` alone, against a chain
//! whose correctness depends on the order somebody wrote it in.
//!
//! Here routing and resolution are one procedure, so one file covers both, and
//! the properties worth pinning are the ones an `if`-chain gets by accident:
//! that markers cannot shadow one another, that a plane refuses a foreign class
//! before it costs a round trip, and that a deployment with no identity
//! provider still resolves the classes that never needed one.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::authenticate::Registry;
use afd_auth::capability::NoCapabilitySource;
use afd_auth::credential::{
    CLI_CREDENTIAL_PREFIX, CredentialKind, Presented, RUNNER_TOKEN_PREFIX, TENANT_API_KEY_PREFIX,
};
use afd_auth::directory::{CredentialRecord, DIGEST_HEX_LEN, Digest, Liveness};
use afd_auth::error::Error;
use afd_auth::mock::{MockCapabilities, MockDirectory, MockVerifier};
use afd_auth::plane::Plane;
use afd_auth::principal::{PersonCredential, Subject};
use afd_auth::scope::{Scope, ScopeSet, parse_claim};
use afd_auth::verifier::{NoVerifier, VerifiedClaims, VerifyError};
use afd_core::id::Uuid7;

const TENANT: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
const RUNNER: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a6c";
const WORKSPACE: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
const SUBJECT: &str = "user_2aXyTest";
/// A body of the shape every minter produces: 32 random bytes as lower-case hex.
const BODY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a valid v7 identifier")
}

fn subject() -> Subject {
    Subject::new(SUBJECT).expect("a non-blank subject")
}

fn present(raw: &str) -> Presented {
    Presented::new(raw).expect("a non-blank credential")
}

fn live_person() -> CredentialRecord {
    CredentialRecord::Person {
        tenant: uuid(TENANT),
        subject: subject(),
        live: Liveness::Live,
    }
}

fn revoked_person() -> CredentialRecord {
    CredentialRecord::Person {
        tenant: uuid(TENANT),
        subject: subject(),
        live: Liveness::Revoked,
    }
}

fn live_machine(degraded: bool) -> CredentialRecord {
    CredentialRecord::Machine {
        runner: uuid(RUNNER),
        degraded,
        live: Liveness::Live,
    }
}

/// Runs a future to completion on a current-thread runtime.
///
/// The crate is not concurrent, so a multi-thread runtime would be claiming a
/// property nothing here has.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

// ── Classification ───────────────────────────────────────────────────────

/// The property the Zig chain holds by authoring order rather than by rule.
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
/// ever authenticate. In the Zig daemon both are wiring questions.
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
/// In the Zig daemon this holds because two `if`s sit above the `orelse`, so it
/// is a claim about statement order. Here the prefixed classes never consult a
/// verifier at all, so there is no order for a future edit to disturb —
/// [`NoVerifier`] is the whole configuration and the test still passes.
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

// ── One procedure, three classes ─────────────────────────────────────────

/// A malformed body is refused before the round trip that could only have said
/// the same thing — for all three classes, not just the one Zig checks.
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
/// had been narrowed to nothing. `clerk_scope_resolver.zig` makes the same
/// choice by treating an absent secret as a fetch failure.
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
        "lower-case hex, the form `api_key.zig::sha256Hex` writes: {digest}"
    );
    // Over the WHOLE presented value, marker included — the Zig daemon hashes
    // `provided`, so hashing the body alone would authenticate nothing.
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

// ── The credential itself ────────────────────────────────────────────────

/// A presented credential never renders, whatever holds it.
///
/// A bearer value in a log IS the credential, and a derived `Debug` on any
/// struct that transitively holds one is how it gets there.
#[test]
fn test_a_presented_credential_never_renders_its_value() {
    let presented = present(&format!("{CLI_CREDENTIAL_PREFIX}{BODY}"));
    let rendered = format!("{presented:?}");
    assert!(!rendered.contains(BODY), "{rendered}");
    assert!(rendered.contains("redacted"), "{rendered}");
    assert_eq!(presented.len(), CLI_CREDENTIAL_PREFIX.len() + BODY.len());
    assert!(!presented.is_empty());
}

/// The header parse matches `bearer.zig` exactly, including its case
/// sensitivity — leniency here would accept what the Zig daemon refuses.
#[test]
fn test_the_header_parse_matches_the_zig_daemons() {
    assert_eq!(
        Presented::from_authorization("Bearer abc")
            .expect("a token")
            .expose(),
        "abc"
    );
    // Untrimmed: the Zig daemon hashes the raw slice, so trimming here would
    // hash different bytes than the column holds.
    assert_eq!(
        Presented::from_authorization("Bearer  abc ")
            .expect("a token")
            .expose(),
        " abc "
    );
    for bad in ["bearer abc", "BEARER abc", "Bearer", "Bearer   ", "abc", ""] {
        assert!(Presented::from_authorization(bad).is_err(), "{bad:?}");
    }
}
