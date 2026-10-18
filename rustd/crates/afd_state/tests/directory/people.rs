//! Dimension 5.5 — a person's credentials resolve, and an unmatched digest is none.
//!
//! The runner class is `runners.rs`; the outage half of the distinction below
//! is `outages.rs`.
//!
//! The assertion this suite exists for is the one about `Ok(None)` versus
//! `Err(Unavailable)`. Every other property here would still hold if those two
//! collapsed into each other, and collapsing them reports a Postgres outage as
//! an authentication rejection — which the runner client counts toward a
//! self-termination ceiling. A datastore blip would walk a healthy fleet's
//! runners to shutdown, one reject at a time.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Liveness};
use afd_state::Credentials;

use crate::support::{Fixtures, digest_of, identifier, per_run};

/// A tenant api-key resolves to the person who minted it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_tenant_api_key_resolves_to_its_minter() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(1)).await;
    let digest = digest_of("agt_t_fixture_live");
    fixtures
        .api_key(&tenant, &digest, "auth0|minter", true)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::TenantApiKey, &digest)
        .await
        .expect("a reachable datastore answers");

    match found.expect("the key was seeded and must resolve") {
        CredentialRecord::Person {
            tenant: resolved,
            subject,
            live,
        } => {
            assert_eq!(resolved.as_str(), tenant);
            assert_eq!(
                subject.as_str(),
                "auth0|minter",
                "created_by holds the provider subject directly, which is why \
                 this class joins nothing"
            );
            assert_eq!(live, Liveness::Live);
        }
        record @ CredentialRecord::Machine { .. } => {
            panic!("a tenant api-key is a person's credential, got {record:?}")
        }
    }

    fixtures.cleanup().await;
}

/// A revoked key comes back as a row, not as nothing.
///
/// Filtering it out in SQL would make it indistinguishable from a key that
/// never existed, and the difference is what lets it answer `UZ-APIKEY-004` —
/// "this is dead, stop" — instead of a bare 401 the holder retries forever.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_revoked_key_resolves_and_says_it_is_revoked() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(2)).await;
    let digest = digest_of("agt_t_fixture_revoked");
    fixtures
        .api_key(&tenant, &digest, "auth0|minter", false)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::TenantApiKey, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person {
                live: Liveness::Revoked,
                ..
            })
        ),
        "a revoked key must resolve as a revoked row, got {found:?}"
    );

    fixtures.cleanup().await;
}

/// A command-line credential joins through to the person's provider subject.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_cli_credential_resolves_through_its_user() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(3)).await;
    let holder = per_run("auth0|holder");
    let user = fixtures.user(&identifier(4), &tenant, &holder).await;
    let digest = digest_of("afc_fixture_live");
    fixtures
        .cli_credential(&user, &tenant, &digest, false)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::CliCredential, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person { ref subject, live: Liveness::Live, .. })
                if subject.as_str() == holder
        ),
        "the join must carry oidc_subject, never the users row id, got {found:?}"
    );

    fixtures.cleanup().await;
}

/// Revocation is the nullness of the timestamp, not its value.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_revoked_cli_credential_resolves_as_revoked() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(5)).await;
    // A subject of its own: `uq_users_oidc_subject` is unique across the whole
    // table, so the sibling test's holder is not free to reuse here.
    let user = fixtures
        .user(&identifier(6), &tenant, &per_run("auth0|holder-revoked"))
        .await;
    let digest = digest_of("afc_fixture_revoked");
    fixtures.cli_credential(&user, &tenant, &digest, true).await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::CliCredential, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person {
                live: Liveness::Revoked,
                ..
            })
        ),
        "got {found:?}"
    );

    fixtures.cleanup().await;
}

/// A digest nothing matches is `Ok(None)`, in all three stores.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_unmatched_digest_is_none_not_an_error() {
    let fixtures = Fixtures::create().await;
    let directory = Credentials::new(fixtures.database().clone());
    let digest = digest_of("agt_t_nothing_matches_this");

    for kind in [
        CredentialKind::TenantApiKey,
        CredentialKind::CliCredential,
        CredentialKind::RunnerToken,
    ] {
        let found = directory
            .resolve(kind, &digest)
            .await
            .expect("an empty result is not a failure");
        assert!(
            found.is_none(),
            "{kind:?} answered {found:?} for a digest no row carries"
        );
    }

    fixtures.cleanup().await;
}

/// A session token is verified, never looked up.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_session_token_is_not_looked_up() {
    let fixtures = Fixtures::create().await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(
            CredentialKind::OidcSessionToken,
            &digest_of("header.body.sig"),
        )
        .await
        .expect("there is no store to be unavailable");

    assert!(
        found.is_none(),
        "a session token has no row to find, and asking must not invent one"
    );

    fixtures.cleanup().await;
}
